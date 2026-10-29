"""An installer payload must never contain or replace persistent user state."""
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from client_host_entry import stage_source


class ClientPayloadTests(unittest.TestCase):
    def bundle(self, folder, files):
        archive = folder / 'source.tar.gz'
        manifest = {'format': 1, 'files': [dict(path=name, size=len(data), sha256=hashlib.sha256(data).hexdigest(), mode=0o644) for name, data in files]}
        with tarfile.open(archive, 'w:gz') as target:
            for name, data in files + [('HOST-MANIFEST.json', json.dumps(manifest).encode())]:
                entry = tarfile.TarInfo(name); entry.size = len(data)
                target.addfile(entry, io.BytesIO(data))
        return archive

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
        for name in ['../outside', '/outside', 'data/chat.txt', '.env', 'scripts/../../bad', 'scripts\\bad']:
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
