"""Fresh-export producer behavior, with actual bounded OCI bytes and no engine."""
from copy import deepcopy
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import build_container_images as producer
import container_images
import test_oci_layer_transform as fixtures


class Export:
    def __init__(self, raw):
        self.stdout=io.BytesIO(raw);self.stopped=False

    def poll(self):
        return 0

    def wait(self, timeout=None):
        return 0


def retained_fixture():
    value=fixtures.fixture()
    for role in ('workspace','desktop'):fixtures.add_proof(value,role)
    metadata={name:value['files'].pop(name) for name in ('index.json','manifest.json')}
    value['files'].update(metadata)
    return value


class FreshProducerTests(unittest.TestCase):
    def run_build(self, root, value, records=None):
        child=Export(fixtures.tar_bytes(value['files']))
        records=deepcopy(value['records'] if records is None else records)
        with patch.object(producer,'daemon_architecture',return_value='arm64'),patch.object(producer,'source_records',return_value=records),patch.object(producer.subprocess,'Popen',return_value=child) as create,patch.object(producer.shutil,'disk_usage',return_value=type('Usage',(),{'free':32*1024**3})()):
            result=producer.build({role:role for role in container_images.ROLES},root/'images.payload')
        self.assertEqual(create.call_count,1)
        self.assertEqual(create.call_args.args[0],['docker','image','save','--platform','linux/arm64',*[row['ref'] for row in value['records'].values()]])
        self.assertTrue(child.stdout.closed)
        self.assertEqual(list(root.glob('.images-build-*')),[])
        return result

    def built(self, root, value):
        result=self.run_build(root,value)
        prepared=container_images.stage(root/'images.payload')
        self.assertEqual(result['sha256'],prepared.identity)
        self.assertEqual(result['images'],prepared.manifest['images'])
        self.assertEqual(prepared.manifest['format'],2)
        self.assertEqual(prepared.manifest['archive']['codec'],'xz')
        with prepared.archive() as packed,fixtures.m.read_tar(packed,container_images.MAX_RAW) as archive:
            names=archive.getnames();files={entry.name:archive.extractfile(entry).read() for entry in archive if entry.isfile()}
        self.assertEqual(len(names),len(set(names)))
        return result,files

    def test_format_detection_does_not_add_a_full_export_decompression(self):
        from build_capsule_codec import CapsuleReader
        restart=CapsuleReader._restart;decodes=[]
        def measured(reader):
            if reader.codec=='gzip':decodes.append(reader.source)
            return restart(reader)
        with tempfile.TemporaryDirectory() as temp,patch.object(CapsuleReader,'_restart',measured):
            self.run_build(Path(temp),fixtures.fixture())
        self.assertEqual(len(decodes),3,'OCI catalog, bound-layer verification and emission need three passes; format detection must reuse catalog facts')

    def test_finished_capsule_is_decompressed_once_before_publication(self):
        value=fixtures.fixture();readers=[];native=tarfile.open
        def open_tar(*args,**kwargs):
            mode=kwargs.get('mode',args[1] if len(args)>1 else 'r')
            if mode in ('r|xz','r:xz'):readers.append(kwargs.get('fileobj'))
            return native(*args,**kwargs)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            with patch('tarfile.open',side_effect=open_tar):
                result=self.run_build(root,value)
            self.assertEqual(len(readers),1,'One final admission must verify the entire finished XZ archive without a duplicate decompression')
            admitted=container_images.stage(root/'images.payload')
            self.assertEqual(result['sha256'],admitted.identity)
            self.assertEqual(result['images'],admitted.manifest['images'])

    def test_final_admission_rejects_changed_compressed_bytes_and_keeps_previous_output(self):
        finish=producer.finish_metadata
        def changed(path,*args):
            manifest=finish(path,*args)
            with path.open('r+b') as output:
                output.seek(512+manifest['archive']['size']//2)
                byte=output.read(1);output.seek(-1,1);output.write(bytes([byte[0]^1]))
            return manifest
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);destination=root/'images.payload';destination.write_bytes(b'previous immutable output')
            with patch.object(producer,'finish_metadata',side_effect=changed),self.assertRaisesRegex(ValueError,'integrity'):
                self.run_build(root,fixtures.fixture())
            self.assertEqual(destination.read_bytes(),b'previous immutable output')
            self.assertEqual(list(root.glob('.images-build-*')),[])

    def test_final_admission_rejects_wrong_role_binding_with_valid_archive_digest(self):
        finish=producer.finish_metadata
        def changed(*args):
            manifest=deepcopy(finish(*args));record=manifest['images']['desktop']
            record['export_digest']='sha256:'+'f'*64
            record['ref']=container_images.image_ref('desktop',record['export_digest'])
            return manifest
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);destination=root/'images.payload';destination.write_bytes(b'previous immutable output')
            with patch.object(producer,'finish_metadata',side_effect=changed),self.assertRaisesRegex(ValueError,'references'):
                self.run_build(root,fixtures.fixture())
            self.assertEqual(destination.read_bytes(),b'previous immutable output')
            self.assertEqual(list(root.glob('.images-build-*')),[])

    def test_fresh_export_converts_only_local_layers_and_publishes_bound_refs(self):
        value=fixtures.fixture();original=deepcopy(value['records'])
        with tempfile.TemporaryDirectory() as temp:
            result,files=self.built(Path(temp),value)
        for role,record in result['images'].items():
            self.assertEqual(record['source_digest'],original[role]['source_digest'])
            self.assertEqual(record['config_digest'],original[role]['config_digest'])
            self.assertEqual(files[fixtures.blob(record['config_digest'])],value['files'][fixtures.blob(original[role]['config_digest'])])
            graph=json.loads(files[fixtures.blob(record['export_digest'])])
            if role in ('workspace','desktop'):
                self.assertNotEqual(record['ref'],original[role]['ref'])
                for layer in graph['layers']:
                    self.assertEqual(layer['mediaType'],fixtures.RAW)
                    self.assertEqual(files[fixtures.blob(layer['digest'])],value['plain'][next(fixtures.blob(item['digest']) for item in value['graphs'][role]['layers'] if fixtures.digest(value['plain'][fixtures.blob(item['digest'])])==layer['digest'])])
            else:self.assertEqual(record,original[role])
        for name in fixtures.preserved_closure(value):self.assertEqual(files[name],value['files'][name])
        self.assertEqual(value['records'],original,'producer must not mutate its source receipt')

    def test_upstream_shared_gzip_remains_identical_in_fresh_capsule(self):
        value=fixtures.fixture(shared_upstream=True)
        with tempfile.TemporaryDirectory() as temp:
            result,files=self.built(Path(temp),value)
        shared=value['graphs']['workspace']['layers'][0]
        self.assertEqual(files[fixtures.blob(shared['digest'])],value['files'][fixtures.blob(shared['digest'])])
        for role in ('workspace','desktop','redis'):
            graph=json.loads(files[fixtures.blob(result['images'][role]['export_digest'])])
            self.assertEqual(graph['layers'][0],shared)
        self.assertEqual(result['images']['workspace'],value['records']['workspace'])

    def test_incorrect_diff_id_keeps_previous_output_without_publication(self):
        value=fixtures.fixture()
        fixtures.replace_role_config(value,'desktop',lambda config:config['rootfs']['diff_ids'].__setitem__(0,'sha256:'+'0'*64))
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);destination=root/'images.payload';destination.write_bytes(b'previous immutable output')
            with self.assertRaises(ValueError):self.run_build(root,value)
            self.assertEqual(destination.read_bytes(),b'previous immutable output')
            self.assertEqual(list(root.glob('.images-build-*')),[])

    def test_inspection_config_mismatch_is_not_silently_rebound(self):
        value=fixtures.fixture();records=deepcopy(value['records']);records['desktop']['config_digest']='sha256:'+'0'*64
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);destination=root/'images.payload';destination.write_bytes(b'previous immutable output')
            with self.assertRaises(ValueError):self.run_build(root,value,records)
            self.assertEqual(destination.read_bytes(),b'previous immutable output')

    def test_raw_output_budget_failure_preserves_previous_output(self):
        value=fixtures.fixture()
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);destination=root/'images.payload';destination.write_bytes(b'previous immutable output')
            with patch.object(producer,'MAX_RAW',1024,create=True),self.assertRaises(ValueError):self.run_build(root,value)
            self.assertEqual(destination.read_bytes(),b'previous immutable output')
            self.assertEqual(list(root.glob('.images-build-*')),[])

    def test_missing_role_binding_never_publishes(self):
        value=fixtures.fixture();files=value['files'];index=value['index']
        index['manifests']=[row for row in index['manifests'] if row.get('annotations',{}).get('io.containerd.image.name')!='docker.io/'+value['records']['desktop']['ref']]
        fixtures.sync_metadata(value)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);destination=root/'images.payload';destination.write_bytes(b'previous immutable output')
            with self.assertRaises(ValueError):self.run_build(root,value)
            self.assertEqual(destination.read_bytes(),b'previous immutable output')

    def test_cancel_during_conversion_keeps_complete_output_and_does_not_replay(self):
        class Cancelled(BaseException):pass
        value=fixtures.fixture();child=Export(fixtures.tar_bytes(value['files']));cancel=Cancelled('fixture cancellation')
        def convert(source,destination,records):
            self.assertTrue(source.seekable());destination.write(b'partial candidate')
            raise cancel
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);destination=root/'images.payload';destination.write_bytes(b'previous immutable output')
            with patch.object(producer,'daemon_architecture',return_value='arm64'),patch.object(producer,'source_records',return_value=deepcopy(value['records'])),patch.object(producer.subprocess,'Popen',return_value=child) as create,patch.object(producer,'transform',side_effect=convert),patch.object(producer.shutil,'disk_usage',return_value=type('Usage',(),{'free':32*1024**3})()),self.assertRaises(Cancelled) as raised:
                producer.build({role:role for role in container_images.ROLES},destination)
            self.assertIs(raised.exception,cancel)
            self.assertEqual(create.call_count,1)
            self.assertTrue(child.stdout.closed)
            self.assertEqual(destination.read_bytes(),b'previous immutable output')
            self.assertEqual(list(root.glob('.images-build-*')),[])


if __name__=='__main__':unittest.main()
