"""Offline image selection never overwrites running-device aliases or user state."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from container_images import checked_manifest,stage,activate,image_environment,inspect_archive

ROLES=('workspace','desktop','postgres','redis')


class ContainerImageTests(unittest.TestCase):
    def fixture(self, root, oci=False):
        configs={};docker_manifest=[];images={}
        for role in ROLES:
            data=json.dumps({'architecture':'arm64','os':'linux','role':role}).encode();sha=hashlib.sha256(data).hexdigest();name=sha+'.json';configs[name]=data;ref=f'ai-phone-bundle/{role}:sha-{sha}'
            images[role]={'source_digest':'sha256:'+sha,'export_digest':'sha256:'+sha,'config_digest':'sha256:'+sha,'ref':ref,'size':100}
            docker_manifest.append({'Config':name,'RepoTags':[ref],'Layers':[]})
        if oci:
            descriptors=[]
            for role,row in zip(ROLES,docker_manifest):
                image=images[role];old=row['Config'];config_data=configs.pop(old);row['Config']='blobs/sha256/'+image['config_digest'][7:];configs[row['Config']]=config_data
                graph=json.dumps({'schemaVersion':2,'config':{'digest':image['config_digest']},'layers':[]}).encode();identity=hashlib.sha256(graph).hexdigest();configs['blobs/sha256/'+identity]=graph;image['export_digest']='sha256:'+identity;image['ref']=f'ai-phone-bundle/{role}:sha-{identity}';row['RepoTags']=[image['ref']]
                descriptors.append({'digest':'sha256:'+identity,'annotations':{'io.containerd.image.name':'docker.io/'+image['ref']}})
            configs['index.json']=json.dumps({'schemaVersion':2,'manifests':descriptors}).encode()
        raw=io.BytesIO()
        with tarfile.open(fileobj=raw,mode='w:gz') as target:
            for name,data in [*configs.items(),('manifest.json',json.dumps(docker_manifest).encode())]:
                entry=tarfile.TarInfo(name);entry.size=len(data);target.addfile(entry,io.BytesIO(data))
        data=raw.getvalue();manifest={'format':1,'os':'linux','architecture':'arm64','redistribution':'review-pending','images':images,'archive':{'path':'images.tar.gz','size':len(data),'sha256':hashlib.sha256(data).hexdigest()}}
        capsule=root/'images.payload'
        with tarfile.open(capsule,'w') as target:
            for name,body in [('IMAGES-MANIFEST.json',json.dumps(manifest).encode()),('images.tar.gz',data)]:
                entry=tarfile.TarInfo(name);entry.size=len(body);target.addfile(entry,io.BytesIO(body))
        return capsule,manifest

    def test_valid_capsule_relocates_without_touching_existing_user_files(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,manifest=self.fixture(root);home=root/'installation';user=home/'user/data';user.mkdir(parents=True);(user/'chat').write_text('keep')
            component=stage(capsule);self.assertEqual(stage(capsule),component)
            with component.archive() as stream:inspect_archive(stream,manifest)
            capsule.write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError,'integrity'):stage(capsule)
            self.assertEqual((user/'chat').read_text(),'keep')

    def test_oci_platform_identity_is_bound_to_the_config_and_alias(self):
        from container_images import inventory
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,manifest=self.fixture(root,oci=True);prepared=stage(capsule)
            with prepared.archive() as source:found=inventory(source)
            self.assertEqual(set(found),{image['ref'] for image in manifest['images'].values()})
            changed={**manifest,'images':{**manifest['images'],'desktop':{**manifest['images']['desktop'],'ref':'ai-phone-bundle/desktop:sha-'+'0'*64}}}
            with self.assertRaises(ValueError):checked_manifest(changed)
            with prepared.archive() as source:
                with tarfile.open(fileobj=source,mode='r:gz') as archive:files={entry.name:archive.extractfile(entry).read() for entry in archive if entry.isfile()}
            index=json.loads(files['index.json']);index['manifests'][0]['annotations']['io.containerd.image.name']='docker.io/ai-phone-desktop:20261028';files['index.json']=json.dumps(index).encode()
            stream=io.BytesIO()
            with tarfile.open(fileobj=stream,mode='w:gz') as archive:
                for name,body in files.items():
                    entry=tarfile.TarInfo(name);entry.size=len(body);archive.addfile(entry,io.BytesIO(body))
            stream.seek(0)
            with self.assertRaisesRegex(ValueError,'Unlisted OCI image'):inventory(stream)

    def test_an_atomic_package_update_does_not_redirect_an_import(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,manifest=self.fixture(root);prepared=stage(capsule);ready={};loaded=[]
            def docker(args,source=None):
                if args[0]=='info':
                    replacement=root/'replacement';replacement.write_bytes(b'untrusted replacement');replacement.replace(capsule)
                    return 'arm64'
                if args[:2]==['image','inspect']:return json.dumps([ready[args[2]]]) if args[2] in ready else None
                if args[:2]==['image','load']:
                    loaded.append(hashlib.file_digest(source,'sha256').hexdigest())
                    for row in manifest['images'].values():ready[row['ref']]={'Id':row['config_digest'],'Architecture':'arm64','Os':'linux'}
                    return ''
                self.fail(args)
            activate(root,prepared,docker)
            self.assertEqual(loaded,[manifest['archive']['sha256']])

    def test_an_inplace_package_change_is_stopped_before_import(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,_=self.fixture(root);prepared=stage(capsule);calls=[]
            def docker(args,source=None):
                calls.append(args)
                if args[0]=='info':capsule.write_bytes(b'changed');return 'arm64'
                if args[:2]==['image','inspect']:return None
                self.fail('Changed capsule was imported')
            with self.assertRaisesRegex(ValueError,'changed during'):activate(root,prepared,docker)
            self.assertFalse((root/'data/deployment/image-runtime.json').exists())

    def test_unknown_partial_import_is_observed_before_retrying(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,manifest=self.fixture(root);prepared=stage(capsule);ready={};attempts=[];observed=[]
            def docker(args,source=None):
                if args[0]=='info':return 'arm64'
                if args[:2]==['image','inspect']:
                    observed.append(args[2]);return json.dumps([ready[args[2]]]) if args[2] in ready else None
                if args[:2]==['image','load']:
                    attempts.append(list(observed));observed.clear()
                    for row in list(manifest['images'].values())[:2 if len(attempts)==1 else 4]:ready[row['ref']]={'Id':row['config_digest'],'Architecture':'arm64','Os':'linux'}
                    if len(attempts)==1:raise ValueError('Unknown import result')
                    return ''
                self.fail(args)
            from container_images import RESERVE
            with patch('container_images.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE})()):
                with self.assertRaisesRegex(ValueError,'Unknown'):activate(root,prepared,docker)
                self.assertFalse((root/'data/deployment/image-runtime.json').exists())
                activate(root,prepared,docker)
            self.assertEqual(len(attempts),2);self.assertEqual(len(attempts[1]),4)

    def test_malformed_headers_paths_sizes_and_records_fail_closed(self):
        from container_images import archive_path,checked_image,checked_archive,outer_manifest,MAX_ARCHIVE
        for path in [None,'','/absolute','a/../b','a//b','a\\b','a'+chr(0)]:
            with self.assertRaises(ValueError):archive_path(path)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);_,manifest=self.fixture(root);image=manifest['images']['desktop']
            for value in [None,[],{}, {**image,'size':True},{**image,'source_digest':'not a digest'}]:
                with self.assertRaises(ValueError):checked_image('desktop',value)
            for value in [None,{}, {**manifest['archive'],'size':MAX_ARCHIVE+1},{**manifest['archive'],'sha256':'bad'}]:
                with self.assertRaises(ValueError):checked_archive(value)
            for names in [[],['extra'],['images.tar.gz','images.tar.gz']]:
                payload=root/'invalid.payload'
                with tarfile.open(payload,'w') as target:
                    for name in names:
                        entry=tarfile.TarInfo(name);entry.size=1;target.addfile(entry,io.BytesIO(b'x'))
                with tarfile.open(payload,'r:') as target,self.assertRaises(ValueError):outer_manifest(target)

    def test_inclusive_size_limits_and_zero_length_members(self):
        from container_images import checked_image,checked_archive,checked_docker_file,archive_path,MAX_ARCHIVE
        with tempfile.TemporaryDirectory() as temp:
            _,manifest=self.fixture(Path(temp));image=manifest['images']['desktop']
            for size in (1,32*1024**3):self.assertEqual(checked_image('desktop',{**image,'size':size})['size'],size)
            for size in (0,-1,32*1024**3+1):
                with self.assertRaises(ValueError):checked_image('desktop',{**image,'size':size})
            for size in (1,MAX_ARCHIVE):checked_archive({**manifest['archive'],'size':size})
            for size in (0,-1,True):
                with self.assertRaises(ValueError):checked_archive({**manifest['archive'],'size':size})
        entry=tarfile.TarInfo('layer.tar')
        for size in (0,MAX_ARCHIVE):entry.size=size;checked_docker_file(entry,set())
        for size in (-1,MAX_ARCHIVE+1):
            entry.size=size
            with self.assertRaises(ValueError):checked_docker_file(entry,set())
        entry.size=0
        with self.assertRaises(ValueError):checked_docker_file(entry,{'layer.tar'})
        entry.type=tarfile.SYMTYPE
        with self.assertRaises(ValueError):checked_docker_file(entry,set())
        self.assertEqual(archive_path('layer one').as_posix(),'layer one')

    def test_outer_headers_stop_before_traversing_untrusted_members(self):
        from container_images import outer_manifest
        def headers(entries):
            yield from entries
            raise AssertionError('Invalid headers must stop traversal')
        valid=tarfile.TarInfo('images.tar.gz');valid.size=1
        extra=tarfile.TarInfo('outside');extra.size=1
        linked=tarfile.TarInfo('images.tar.gz');linked.type=tarfile.SYMTYPE
        empty=tarfile.TarInfo('images.tar.gz');empty.size=0
        for entries in ([extra],[linked],[valid,valid],[empty]):
            with self.subTest(entries=entries),self.assertRaises(ValueError):outer_manifest(headers(entries))
        metadata=tarfile.TarInfo('IMAGES-MANIFEST.json');metadata.size=4
        with patch('container_images.MAX_METADATA',3),patch('container_images.MAX_ARCHIVE',4),self.assertRaises(ValueError):outer_manifest(headers([metadata]))

    def test_each_archive_identity_is_independently_required(self):
        from copy import deepcopy
        with tempfile.TemporaryDirectory() as temp:
            payload,manifest=self.fixture(Path(temp));prepared=stage(payload)
            for field in ('config_digest','export_digest'):
                changed=deepcopy(manifest);changed['images']['desktop'][field]='sha256:'+'f'*64
                with prepared.archive() as source,self.assertRaisesRegex(ValueError,'identity mismatch'):inspect_archive(source,changed)
            changed={**manifest,'architecture':'amd64'}
            with prepared.archive() as source,self.assertRaisesRegex(ValueError,'identity mismatch'):inspect_archive(source,changed)

    def test_outer_manifest_accepts_exact_metadata_and_archive_limits(self):
        from container_images import outer_manifest,MANIFEST,ARCHIVE
        with tempfile.TemporaryDirectory() as temp:
            payload,manifest=self.fixture(Path(temp))
            with tarfile.open(payload,'r:') as source:
                with patch('container_images.MAX_METADATA',source.getmember(MANIFEST).size),patch('container_images.MAX_ARCHIVE',source.getmember(ARCHIVE).size):
                    self.assertEqual(outer_manifest(source),manifest)

    def test_inventory_rejects_shape_and_count_before_visiting_rows(self):
        from container_images import inventory
        class UntrustedRows(dict):
            def __iter__(self):raise AssertionError('Wrong-shaped rows must not be visited')
        class TooManyRows(list):
            def __iter__(self):raise AssertionError('Excess rows must not be visited')
        for rows in ([],UntrustedRows(one='invalid'),TooManyRows([None]*5)):
            with patch('container_images.archive_metadata',return_value=({'manifest.json':rows},set())),self.assertRaisesRegex(ValueError,'inventory'):inventory(None)

    def test_each_inspection_field_and_index_shape_is_validated(self):
        from container_images import present,bind_indexes
        with tempfile.TemporaryDirectory() as temp:
            _,manifest=self.fixture(Path(temp));record=manifest['images']['desktop'];base={'Id':record['export_digest'],'Architecture':'arm64','Os':'linux'}
            for rows in ([],{'unexpected':'object'},[{**base,'Architecture':'amd64'}],[{**base,'Os':'windows'}]):
                with self.subTest(rows=rows),self.assertRaises(ValueError):present(record,'arm64',lambda _:json.dumps(rows))
        with self.assertRaises(ValueError):bind_indexes({'schemaVersion':2,'manifests':None},{},{})

    def test_nested_metadata_limits_and_both_object_naming_formats(self):
        from container_images import archive_metadata,read_object
        stream=io.BytesIO()
        with tarfile.open(fileobj=stream,mode='w:gz') as archive:
            for name in ('one.json','two.json'):
                entry=tarfile.TarInfo(name);entry.size=2;archive.addfile(entry,io.BytesIO(b'{}'))
        for limit,accepted in ((4,True),(3,False)):
            stream.seek(0)
            with patch('container_images.MAX_ARCHIVE',limit),patch('container_images.MAX_METADATA',4):
                if accepted:self.assertEqual(set(archive_metadata(stream)[0]),{'one.json','two.json'})
                else:
                    with self.assertRaises(ValueError):archive_metadata(stream)
        identity=hashlib.sha256(b'{}').hexdigest()
        for name in (identity+'.json','blobs/sha256/'+identity):
            class Archive:
                def extractfile(self,_):return io.BytesIO(b'{}')
            entry=tarfile.TarInfo(name);entry.size=2
            with patch('container_images.MAX_METADATA',2):self.assertEqual(read_object(Archive(),entry),{})
            entry.name=name.replace(identity,'0'*64)
            with self.assertRaisesRegex(ValueError,'integrity'):read_object(Archive(),entry)

    def test_oci_descriptors_reject_foreign_subjects_and_wrong_config_binding(self):
        from container_images import bind_indexes
        record={'config_digest':'sha256:'+'a'*64,'export_digest':'sha256:'+'a'*64};found={'ai-phone-bundle/desktop:sha-test':record}
        for value in [[],{}, {'schemaVersion':2,'manifests':[False]}, {'schemaVersion':2,'manifests':[{'annotations':{}}]}]:
            with self.assertRaises(ValueError):bind_indexes(value,{},found)
        graph={'config':{'digest':'sha256:'+'a'*64}};data=json.dumps(graph).encode();identity=hashlib.sha256(data).hexdigest();objects={'blobs/sha256/'+identity:graph}
        named={'digest':'sha256:'+identity,'annotations':{'io.containerd.image.name':'docker.io/ai-phone-bundle/desktop:sha-test'}}
        bind_indexes({'schemaVersion':2,'manifests':[named,{'annotations':{'io.containerd.manifest.subject':'sha256:'+identity}}]},objects,found)
        with self.assertRaises(ValueError):bind_indexes({'schemaVersion':2,'manifests':[named]}, {'blobs/sha256/'+identity:{'config':{'digest':'sha256:'+'b'*64}}},found)

    def test_directory_flood_and_legacy_aliases_are_bounded(self):
        from container_images import archive_metadata
        for flood in (False,True):
            stream=io.BytesIO()
            with tarfile.open(fileobj=stream,mode='w:gz') as target:
                if flood:
                    for i in range(20001):
                        entry=tarfile.TarInfo('dir'+str(i));entry.type=tarfile.DIRTYPE;target.addfile(entry)
                else:
                    entry=tarfile.TarInfo('repositories');entry.size=2;target.addfile(entry,io.BytesIO(b'{}'))
            stream.seek(0)
            with self.assertRaisesRegex(ValueError,'entry budget' if flood else 'Legacy'):archive_metadata(stream)

    def test_platform_indexes_need_a_verified_selected_manifest(self):
        from container_images import present
        with tempfile.TemporaryDirectory() as temp:
            _,manifest=self.fixture(Path(temp));row=manifest['images']['desktop'];calls=[]
            def docker(args):
                calls.append(args)
                if '--platform' in args:return json.dumps([{'Id':row['export_digest'],'Architecture':'arm64','Os':'linux'}])
                return json.dumps([{'Id':'sha256:'+'f'*64,'Architecture':'arm64','Os':'linux','Descriptor':{'mediaType':'application/vnd.oci.image.index.v1+json'}}])
            self.assertTrue(present(row,'arm64',docker));self.assertIn('--platform',calls[-1])
            with self.assertRaisesRegex(ValueError,'verify'):present(row,'arm64',lambda args:None if '--platform' in args else docker(args))

    def test_real_subprocess_transport_and_failed_publication_keep_owned_state(self):
        import os,shlex,sys
        from container_images import docker_command
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);binary=root/'docker';receipt=root/'received'
            code="import sys,pathlib; arg=sys.argv[1:]; data=sys.stdin.buffer.read() if 'load' in arg else b''; pathlib.Path("+repr(str(receipt))+").write_bytes(data); print('arm64'); sys.exit(3 if 'fail' in arg else 0)"
            binary.write_text('#!/bin/sh\nexec '+shlex.quote(sys.executable)+' -c '+shlex.quote(code)+' "$@"\n');binary.chmod(0o755)
            with patch.dict(os.environ,{'PATH':str(root)+os.pathsep+os.environ['PATH']}):
                self.assertEqual(docker_command(['info']).strip(),'arm64');self.assertIsNone(docker_command(['fail']))
                self.assertEqual(docker_command(['image','load'],io.BytesIO(b'owned image stream')),'');self.assertEqual(receipt.read_bytes(),b'owned image stream')
                with self.assertRaises(ValueError):docker_command(['image','load','fail'],io.BytesIO(b'rejected'))
                import subprocess
                original=subprocess.Popen;children=[]
                def child(*args,**kwargs):
                    value=original(*args,**kwargs);children.append(value);return value
                class BrokenInput:
                    def read(self,_):raise OSError('fixture lost input')
                with patch('container_images.subprocess.Popen',side_effect=child),self.assertRaisesRegex(OSError,'lost input'):docker_command(['image','load'],BrokenInput())
                self.assertTrue(children[0].stdin.closed);self.assertIsNotNone(children[0].poll())

    def test_builder_emits_a_valid_capsule_through_a_real_export_process(self):
        import os,shlex,sys,subprocess
        from build_container_images import build
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,manifest=self.fixture(root);prepared=stage(capsule)
            with prepared.archive() as source:raw=gzip.decompress(source.read())
            (root/'export.tar').write_bytes(raw);configs={role:{'Id':record['config_digest'],'Os':'linux','Architecture':'arm64','Size':100} for role,record in manifest['images'].items()}
            code="import json,pathlib,sys; a=sys.argv[1:]; root=pathlib.Path("+repr(str(root))+"); images="+repr(configs)+"\n"
            code+="if a[0]=='info': print('arm64')\nelif a[:2]==['image','inspect']:\n name=a[2]; item=images.get(name) or next((v for v in images.values() if v['Id']==name),None)\n if item is None: sys.exit(1)\n print(json.dumps([item]))\nelif a[0]=='tag': sys.exit(9 if (root/'fail-tag').exists() else 0)\nelif a[:2]==['image','save']: sys.stdout.buffer.write((root/'export.tar').read_bytes())\nelse: sys.exit(4)\n"
            binary=root/'docker';binary.write_text('#!/bin/sh\nexec '+shlex.quote(sys.executable)+' -c '+shlex.quote(code)+' "$@"\n');binary.chmod(0o755)
            with patch.dict(os.environ,{'PATH':str(root)+os.pathsep+os.environ['PATH']}):
                from container_images import RESERVE
                output=root/'new'/'nested'/'built.payload'
                with patch('build_container_images.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE})()):result=build({role:role for role in ROLES},output)
                size=stage(output).manifest['archive']['size']
                with patch('build_container_images.MAX_ARCHIVE',size):self.assertEqual(build({role:role for role in ROLES},output)['sha256'],result['sha256'])
                (root/'fail-tag').touch()
                with self.assertRaises(subprocess.CalledProcessError):build({role:role for role in ROLES},output)
            built=stage(output);self.assertEqual(result['sha256'],built.identity);self.assertEqual(built.manifest['images'],manifest['images']);self.assertEqual(output.stat().st_size%512,0)
            self.assertLess(built.manifest['archive']['size'],len(raw),'Compressible image data was not compressed')

    def test_bad_build_inputs_stop_before_creating_output_or_querying_docker(self):
        from build_container_images import build
        with tempfile.TemporaryDirectory() as temp,patch('build_container_images.docker_command') as docker:
            output=Path(temp)/'new'/'images.payload'
            for value in (None,{},list(ROLES)):
                with self.assertRaises(ValueError):build(value,output)
            self.assertFalse(output.parent.exists());docker.assert_not_called()

    def test_stream_export_rechecks_space_and_stops_before_the_next_write(self):
        from build_container_images import ArchiveWriter
        from container_images import RESERVE
        class Sink:
            def __init__(self):self.bytes=0
            def write(self,data):self.bytes+=len(data);return len(data)
        block=b'x'*(1024**2);last=block[:-1]
        for free in (RESERVE,RESERVE-1):
            sink=Sink();writer=ArchiveWriter(sink,Path('.'))
            with patch('build_container_images.shutil.disk_usage',return_value=type('Usage',(),{'free':free})()) as space:
                for _ in range(31):writer.write(block)
                writer.write(last);space.assert_not_called()
                if free<RESERVE:
                    with self.assertRaisesRegex(ValueError,'4 GiB'):writer.write(b'x')
                    self.assertEqual(sink.bytes,32*1024**2-1)
                else:
                    self.assertEqual(writer.write(b'x'),1);space.assert_called_once()
                    for _ in range(31):writer.write(block)
                    writer.write(last);space.assert_called_once()

    def test_source_image_conflicts_and_architecture_errors_never_retag(self):
        from build_container_images import source_records
        from container_images import image_ref
        sources={role:'mutable-'+role for role in ROLES};image={'Id':'sha256:'+'a'*64,'Architecture':'arm64','Os':'linux','Size':100};primary='sha256:'+'b'*64
        for failure in ['missing','architecture','platform','conflict']:
            def query(args):
                if args[2].startswith('ai-phone-bundle/'):
                    return json.dumps([{**image,'Id':'sha256:'+'c'*64}]) if failure=='conflict' else None
                if '--platform' in args:return None if failure=='platform' else json.dumps([{**image,'Id':primary}])
                return None if failure=='missing' else json.dumps([{**image,'Architecture':'amd64' if failure=='architecture' else 'arm64'}])
            with patch('build_container_images.docker_command',side_effect=query),patch('build_container_images.subprocess.run') as tag:
                with self.assertRaises(ValueError):source_records(sources,'arm64')
                tag.assert_not_called()
        def success(args):
            if args[2].startswith('ai-phone-bundle/'):return None
            return json.dumps([{**image,'Id':primary if '--platform' in args else image['Id']}])
        with patch('build_container_images.docker_command',side_effect=success),patch('build_container_images.subprocess.run') as tag:
            records=source_records(sources,'arm64')
            self.assertEqual(records['desktop']['ref'],image_ref('desktop',primary))
            self.assertTrue(all(call.args[0][2]==image['Id'] for call in tag.call_args_list))

    def test_low_space_and_failed_selection_never_replace_existing_state(self):
        from container_images import RESERVE,atomic_selection
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,manifest=self.fixture(root);prepared=stage(capsule);old=root/'data/deployment';old.mkdir(parents=True);selected=old/'image-runtime.json';selected.write_text('previous selection')
            def missing(args,source=None):
                if args[0]=='info':return 'arm64'
                if args[:2]==['image','inspect']:return None
                self.fail('No import should start without disk reserve')
            with patch('container_images.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE-1})()):
                with self.assertRaisesRegex(ValueError,'4 GiB'):activate(root,prepared,missing)
            with patch('container_images.os.replace',side_effect=OSError('fixture write failure')):
                with self.assertRaises(OSError):atomic_selection(root,manifest)
            self.assertEqual(selected.read_text(),'previous selection');self.assertEqual(list(old.iterdir()),[selected])

    def test_archive_config_rejects_missing_layers_and_invalid_aliases(self):
        from container_images import config_entry
        identity='a'*64;path=identity+'.json';row={'Config':path,'Layers':['layer.tar'],'RepoTags':['owned']};objects={path:{'os':'linux','architecture':'arm64'}}
        for changed in [None,{}, {**row,'Config':'../private'}, {**row,'Layers':None}, {**row,'Layers':['missing']}, {**row,'RepoTags':None}, {**row,'RepoTags':[42]}]:
            with self.subTest(row=changed),self.assertRaises(ValueError):config_entry(changed,objects,{'layer.tar'})
        for config in [None,[],{'os':'windows'}]:
            with self.subTest(config=config),self.assertRaises(ValueError):config_entry(row,{path:config},{'layer.tar'})
        record,refs=config_entry(row,objects,{'layer.tar'})
        self.assertEqual(record,{'config_digest':'sha256:'+identity,'export_digest':'sha256:'+identity,'architecture':'arm64'});self.assertEqual(refs,['owned'])

    def test_archive_metadata_has_an_aggregate_memory_budget(self):
        from container_images import archive_metadata
        stream=io.BytesIO()
        with tarfile.open(fileobj=stream,mode='w:gz') as archive:
            for name in ('one.json','two.json'):
                entry=tarfile.TarInfo(name);entry.size=2;archive.addfile(entry,io.BytesIO(b'{}'))
        stream.seek(0)
        with patch('container_images.MAX_METADATA',3),self.assertRaisesRegex(ValueError,'metadata budget'):archive_metadata(stream)

    def test_an_existing_export_identity_is_not_retagged(self):
        from build_container_images import source_records
        image={'Id':'sha256:'+'a'*64,'Architecture':'arm64','Os':'linux','Size':123}
        with patch('build_container_images.docker_command',return_value=json.dumps([image])),patch('build_container_images.subprocess.run') as tag:
            result=source_records({role:'source-'+role for role in ROLES},'arm64')
        self.assertEqual(set(result),set(ROLES));self.assertTrue(all(row['size']==123 for row in result.values()));tag.assert_not_called()

    def test_cancellation_reaps_a_child_that_ignores_termination(self):
        import subprocess,sys
        from container_images import stop_child
        child=subprocess.Popen([sys.executable,'-c',"import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);print('ready',flush=True);time.sleep(60)"],stdout=subprocess.PIPE)
        original_wait=child.wait;waits=[]
        def simulated_deadline(timeout=None):
            waits.append(timeout)
            if len(waits)==1:raise subprocess.TimeoutExpired(child.args,timeout)
            return original_wait(timeout=timeout)
        try:
            self.assertEqual(child.stdout.readline().strip(),b'ready')
            with patch.object(child,'wait',side_effect=simulated_deadline):stop_child(child)
            self.assertIsNotNone(child.poll());stop_child(child)
        finally:
            if child.poll() is None:child.kill();original_wait()
            child.stdout.close()

    def test_manifest_cannot_name_foreign_tags_architectures_or_private_paths(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);_,manifest=self.fixture(root)
            for field,value in [('format',True),('architecture','mips'),('os','windows'),('images',{}),('redistribution','cleared')]:
                with self.assertRaises(ValueError):checked_manifest({**manifest,field:value})
            for path in ['../user/chat','/outside','images//file','image.tar.gz']:
                with self.assertRaises(ValueError):checked_manifest({**manifest,'archive':{**manifest['archive'],'path':path}})
            for role in ROLES:
                changed={**manifest['images'],role:{**manifest['images'][role],'ref':'ai-phone-desktop:20261028'}}
                with self.assertRaises(ValueError):checked_manifest({**manifest,'images':changed})

    def test_import_failure_does_not_publish_selection_and_resume_never_reloads(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,manifest=self.fixture(root);component=stage(capsule);calls=[];present={}
            def docker(args,source=None):
                calls.append(args)
                if args==['info','--format','{{.Architecture}}']:return 'aarch64'
                if args[:2]==['image','inspect']:return json.dumps([present[args[2]]]) if args[2] in present else None
                if args[:2]==['image','load']:raise ValueError('fixture import failure')
                raise AssertionError(args)
            with self.assertRaisesRegex(ValueError,'import failure'):activate(root,component,docker)
            self.assertFalse((root/'data/deployment/image-runtime.json').exists())
            for image in manifest['images'].values():present[image['ref']]={'Id':image['source_digest'],'Architecture':'arm64','Os':'linux'}
            calls.clear();activate(root,component,docker)
            env=image_environment(root);self.assertEqual(env['AI_PHONE_DESKTOP_IMAGE'],manifest['images']['desktop']['ref']);self.assertEqual(env['AI_PHONE_IMAGE_PULL_POLICY'],'never')
            self.assertNotIn('load',[arg[1] for arg in calls if len(arg)>1])
            calls.clear();activate(root,component,docker);self.assertFalse(any(arg[:2]==['image','load'] for arg in calls))

    def test_daemon_or_existing_reference_mismatch_stops_before_import(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);capsule,_=self.fixture(root);component=stage(capsule)
            for arch in ['x86_64','unknown']:
                with self.assertRaises(ValueError):activate(root,component,lambda args:arch if args[0]=='info' else self.fail('Unexpected Docker side effect'))
            def mismatch(args,source=None):
                if args[0]=='info':return 'arm64'
                if args[:2]==['image','inspect']:return json.dumps([{'Id':'sha256:'+'0'*64,'Architecture':'arm64','Os':'linux'}])
                self.fail('Must not overwrite a conflicting reference')
            with self.assertRaisesRegex(ValueError,'conflict'):activate(root,component,mismatch)


if __name__=='__main__':unittest.main()
