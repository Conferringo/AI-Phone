"""Untrusted runtime metadata cannot redirect execution or overwrite private state."""
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from client_host_entry import stage_source,source_name,web_links
from prebuilt_web import web_command,public_runtime_file
from host_platform import host_key
from prepare_client_host import payload_components


class RuntimeIntegrityTests(unittest.TestCase):
    def runtime(self, root):
        files={'runtime/bin/'+('bun.exe' if os.name=='nt' else 'bun'):b'fixture executable','runtime/web/apps/web/server.js':b'fixture server'}
        for name,data in files.items():
            p=root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
        manifest={'format':1,'host':host_key(),'services':['web'],'links':[],'files':[{'path':name,'size':len(data),'sha256':hashlib.sha256(data).hexdigest(),'mode':0o755} for name,data in files.items()]}
        self.save(root,manifest);return manifest
    def save(self, root, manifest):
        p=root/'runtime/manifest.json';p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(manifest))
    def archive(self, root, manifest, extra=[]):
        p=root/'source.payload'
        with tarfile.open(p,'w:gz') as target:
            for name,data in [('HOST-MANIFEST.json',json.dumps(manifest).encode()),*extra]:
                entry=tarfile.TarInfo(name);entry.size=len(data);target.addfile(entry,io.BytesIO(data))
        return p
    def test_payload_component_metadata_matches_the_actual_resource(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.assertEqual(payload_components(root),[])
            archive=self.archive(root,{'format':2,'components':['web'],'files':[]})
            target=root/'apps/client/src-tauri/gen/host/source.payload';target.parent.mkdir(parents=True);archive.rename(target)
            self.assertEqual(payload_components(root),['web'])

    def test_noncanonical_and_private_paths_are_rejected_on_every_host(self):
        self.assertEqual(source_name('docs/a b.md').as_posix(),'docs/a b.md')
        for name in ['',None,'.','C:outside','C:/outside','/outside','a\\b','a/../b','a//b','a/./b','a\x00b','.git/config','data/token','user/chat','runtime/bin/bun']:
            with self.subTest(name=name),self.assertRaises(ValueError):source_name(name)
        for name in ['runtime','target/cache.bin','target/web','node_modules/bin']:
            with self.assertRaises(ValueError):source_name(name,runtime=True)
        with self.assertRaises(ValueError):source_name('runtime/web/data/token',runtime=True)
    def test_schema_failures_do_not_modify_existing_user_files(self):
        cases=[None,{'format':True,'files':[]},{'format':3,'files':[]},{'format':1,'files':None},{'format':1,'files':[None]},{'format':1,'files':[{'path':'x','size':True,'mode':420,'sha256':'0'*64}]},{'format':1,'files':[{'path':'x','size':-1,'mode':420,'sha256':'0'*64}]},{'format':1,'files':[{'path':'x','size':0,'mode':777,'sha256':'0'*64}]},{'format':1,'files':[{'path':'x','size':0,'mode':420,'sha256':'invalid'}]}]
        for manifest in cases:
            with self.subTest(manifest=manifest),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);installation=root/'install';user=installation/'user/data';user.mkdir(parents=True);(user/'chat').write_bytes(b'keep')
                with self.assertRaises(ValueError):stage_source(self.archive(root,manifest),installation)
                self.assertEqual((user/'chat').read_bytes(),b'keep')
    def test_duplicate_or_missing_archive_inventory_is_not_activated(self):
        row={'path':'a','size':1,'mode':420,'sha256':hashlib.sha256(b'a').hexdigest()}
        for rows,extra in [([row,row],[('a',b'a')]),([row],[]),([],[('a',b'a')]),([],[('HOST-MANIFEST.json',b'{}')])]:
            with tempfile.TemporaryDirectory() as temp:
                root=Path(temp)
                with self.assertRaises(ValueError):stage_source(self.archive(root,{'format':1,'files':rows},extra),root/'install')
    def test_runtime_host_services_inventory_and_file_integrity_are_bound(self):
        for field,value in [('host','different-host'),('format',2),('services',['unknown']),('files',[]),('files',None)]:
            with self.subTest(field=field),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);manifest=self.runtime(root);manifest[field]=value;self.save(root,manifest)
                with self.assertRaises(ValueError):web_command(root)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);manifest=self.runtime(root);self.assertEqual(len(web_command(root)),2)
            manifest['files'].append(manifest['files'][0]);self.save(root,manifest)
            with self.assertRaisesRegex(ValueError,'Duplicate'):web_command(root)
            manifest['files'].pop();self.save(root,manifest)
            file=root/manifest['files'][1]['path'];file.write_bytes(b'x'*file.stat().st_size)
            with self.assertRaisesRegex(ValueError,'integrity'):web_command(root)
            file.write_bytes(b'fixture server');(root/manifest['files'][0]['path']).unlink()
            with self.assertRaisesRegex(ValueError,'integrity'):web_command(root)
    def test_runtime_aliases_cannot_escape_cycle_or_replace_files(self):
        invalid=[None,{}, {'path':'runtime/web/a','target':4},{'path':'source/a','target':'.'},{'path':'runtime/web/a','target':'/outside'},{'path':'runtime/web/a','target':'C:/outside'},{'path':'runtime/web/a','target':'../../outside'},{'path':'runtime/web/a','target':'.'},{'path':'runtime/web/a','target':'missing'}]
        for row in invalid:
            with self.subTest(row=row),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);manifest=self.runtime(root);(root/'outside').write_bytes(b'private fixture');manifest['links']=[row];self.save(root,manifest)
                with self.assertRaises(ValueError):web_links(root,create=True)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);manifest=self.runtime(root)
            manifest['links']=[{'path':'runtime/web/absolute-alias','target':str((root/'runtime/web/apps/web/server.js').resolve())}];self.save(root,manifest)
            with self.assertRaises(ValueError):web_links(root,create=True)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);manifest=self.runtime(root);manifest['links']=[{'path':'runtime/web/apps/web/server.js','target':'../../apps/web/server.js'}];self.save(root,manifest)
            with self.assertRaisesRegex(ValueError,'replaced'):web_links(root,create=True)
    def test_vendor_examples_are_preserved_but_local_paths_or_modified_copies_fail(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);relative=Path('node_modules/example/index.js');upstream=root/relative;upstream.parent.mkdir(parents=True)
            public_path='/'+'Users/'+'upstream/'
            data=('// '+public_path+'example').encode();upstream.write_bytes(data);source=root/'copy.js';source.write_bytes(data)
            self.assertEqual(public_runtime_file(root,source,relative,'runtime/web/'+relative.as_posix()),data)
            source.write_bytes(data+b' altered')
            with self.assertRaisesRegex(ValueError,'Private'):public_runtime_file(root,source,relative,'runtime/web/'+relative.as_posix())
            local=str(Path.home()).encode()+b'/private';upstream.write_bytes(local);source.write_bytes(local)
            if os.name!='nt':
                with self.assertRaisesRegex(ValueError,'Private'):public_runtime_file(root,source,relative,'runtime/web/'+relative.as_posix())

if __name__=='__main__':unittest.main()
