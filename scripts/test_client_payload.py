"""An installer payload must never contain or replace persistent user state."""
import hashlib
import contextlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from client_host_entry import stage_source
from prepare_client_host import prepare


class ClientPayloadTests(unittest.TestCase):
    def bundle(self, folder, files):
        archive = folder / 'source.payload'
        manifest = {'format': 1, 'files': [dict(path=name, size=len(data), sha256=hashlib.sha256(data).hexdigest(), mode=0o644) for name, data in files]}
        with tarfile.open(archive, 'w:gz') as target:
            for name, data in files + [('HOST-MANIFEST.json', json.dumps(manifest).encode())]:
                entry = tarfile.TarInfo(name); entry.size = len(data)
                target.addfile(entry, io.BytesIO(data))
        return archive

    def test_native_payload_has_an_opaque_name_and_exact_compressed_bytes(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'scripts').mkdir();(root/'scripts/client_host_entry.py').write_text('fixture entry')
            digests=prepare(root)
            self.assertIn('source.payload',digests)
            archive=root/'apps/client/src-tauri/gen/host/source.payload'
            self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(),digests['source.payload'])
            with tarfile.open(archive,'r:gz') as source:self.assertIn('scripts/client_host_entry.py',source.getnames())

    def test_prebuilt_runtime_paths_require_the_explicit_payload_contract(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);archive=self.bundle(root,[('runtime/web/node_modules/package/data/index.js',b'public runtime'),('runtime/manifest.json',b'{"format":1,"links":[]}')])
            with self.assertRaises(ValueError):stage_source(archive,root/'installation')
            with tarfile.open(archive,'r:gz') as source:
                records=[(item.name,source.extractfile(item).read()) for item in source.getmembers()]
            with tarfile.open(archive,'w:gz') as target:
                for name,data in records:
                    if name=='HOST-MANIFEST.json':
                        manifest=json.loads(data);manifest.update(format=2,components=['web']);data=json.dumps(manifest).encode()
                    entry=tarfile.TarInfo(name);entry.size=len(data);target.addfile(entry,io.BytesIO(data))
            staged=stage_source(archive,root/'installation')
            self.assertEqual((staged/'runtime/web/node_modules/package/data/index.js').read_bytes(),b'public runtime')
            (staged/'data/chat.txt').write_text('retained')
            self.assertEqual(stage_source(archive,root/'installation'),staged)
            self.assertEqual((staged/'data/chat.txt').read_text(),'retained')

    def test_retry_reuses_immutable_source_and_preserves_user_state(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); archive = self.bundle(root, [('scripts/example.py', b'print(1)')])
            source = stage_source(archive, root / 'installation')
            (source / 'data/chat.txt').write_text('keep chat')
            (source / '.env').write_text('keep credentials')
            self.assertEqual(stage_source(archive, root / 'installation'), source)
            self.assertEqual((source / 'data/chat.txt').read_text(), 'keep chat')
            self.assertEqual((source / '.env').read_text(), 'keep credentials')
            (source / 'scripts/example.py').write_text('tampered')
            with self.assertRaisesRegex(ValueError, 'changed'):
                stage_source(archive, root / 'installation')

    def test_traversal_private_state_and_symlinks_are_rejected(self):
        for name in ['../outside', '/outside', 'C:/outside', 'data/chat.txt', '.env', 'scripts/../../bad', 'scripts\\bad']:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temp:
                root = Path(temp); archive = self.bundle(root, [(name, b'bad')])
                with self.assertRaises(ValueError): stage_source(archive, root / 'installation')
                self.assertFalse((root / 'outside').exists())
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); archive = self.bundle(root, [('scripts/example.py', b'content')])
            with tarfile.open(archive, 'w:gz') as target:
                entry = tarfile.TarInfo('scripts/example.py'); entry.type = tarfile.SYMTYPE; entry.linkname = '/tmp/unsafe'
                target.addfile(entry)
            with self.assertRaises(ValueError): stage_source(archive, root / 'installation')

    def test_relative_installation_path_keeps_persistent_links_resolvable(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);archive=self.bundle(root,[('scripts/example.py',b'code')])
            with contextlib.chdir(root):source=stage_source(archive,Path('installation'))
            self.assertTrue((source/'data').is_dir())
            (source/'data/chat.txt').write_text('keep')
            self.assertEqual((root/'installation/user/data/chat.txt').read_text(),'keep')

    def test_partial_staging_does_not_activate_and_upgrade_keeps_one_user_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); archive = self.bundle(root, [('scripts/example.py', b'one')])
            source = stage_source(archive, root / 'installation')
            (source / 'data/chat.txt').write_text('keep')
            archive = self.bundle(root, [('scripts/example.py', b'two')])
            upgraded = stage_source(archive, root / 'installation')
            self.assertNotEqual(source, upgraded)
            self.assertEqual((upgraded / 'data/chat.txt').read_text(), 'keep')
            self.assertEqual((source / 'scripts/example.py').read_text(), 'one')
            self.assertFalse((root / 'installation/.staging').exists())


if __name__ == '__main__': unittest.main()
