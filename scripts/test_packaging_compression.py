"""Fresh compressed resources retain legacy imports and one persistent user home."""
import gzip
import io
import lzma
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import android_runtime
import build_container_images
import container_images
import prepare_client_host
import test_android_runtime
import test_client_payload
import test_container_images
from test_fresh_images_producer import export_process
from client_host_entry import stage_source


class Export:
    def __init__(self, data):
        self.stdout = io.BytesIO(data)

    def wait(self, **kwargs):
        return 0

    def poll(self):
        return 0


class PackagingCompressionTests(unittest.TestCase):
    def test_fresh_source_uses_xz_and_preserves_the_existing_user_home(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'scripts').mkdir()
            (root / 'scripts/client_host_entry.py').write_text('fixture entry')
            legacy = test_client_payload.ClientPayloadTests().bundle(root, [('scripts/example.py', b'old source')])
            first = stage_source(legacy, root / 'installation')
            (first / 'data/chat.txt').write_text('retained chat')
            prepare_client_host.prepare(root)
            payload = root / 'apps/client/src-tauri/gen/host/source.payload'
            self.assertEqual(payload.read_bytes()[:6], b'\xfd7zXZ\x00')
            source = stage_source(payload, root / 'installation')
            self.assertEqual((source / 'data/chat.txt').read_text(), 'retained chat')
            self.assertEqual((source / 'scripts/client_host_entry.py').read_text(), 'fixture entry')
            self.assertNotEqual(first, source)
            self.assertEqual(stage_source(payload, root / 'installation'), source)

    def test_xz_android_is_admitted_without_replacing_any_existing_device_data(self):
        with tempfile.TemporaryDirectory() as temp, patch('android_runtime.host_key', return_value='macos-arm64'):
            root = Path(temp)
            payload, manifest = test_android_runtime.AndroidRuntimeTests().fixture(root)
            legacy = android_runtime.stage(payload, root / 'installation')
            user = root / 'installation/user/data'
            user.mkdir(parents=True, exist_ok=True)
            (user / 'chat.txt').write_text('keep')
            payload.write_bytes(lzma.compress(gzip.decompress(payload.read_bytes())))
            compressed = android_runtime.stage(payload, root / 'installation')
            self.assertNotEqual(legacy, compressed)
            self.assertEqual(android_runtime.verify_sdk(compressed)['files'], manifest['files'])
            self.assertEqual((user / 'chat.txt').read_text(), 'keep')
            self.assertEqual(android_runtime.stage(payload, root / 'installation'), compressed)

    def test_fresh_classic_docker_export_uses_xz_without_changing_the_raw_tar(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            capsule, manifest = test_container_images.ContainerImageTests().fixture(root)
            with container_images.stage(capsule).archive() as incoming:
                raw = gzip.decompress(incoming.read())
            child = Export(raw)
            with patch.object(build_container_images, 'daemon_architecture', return_value='arm64'), \
                 patch.object(build_container_images, 'source_records', return_value=manifest['images']), \
                 export_process(child):
                receipt = build_container_images.build({role: role for role in container_images.ROLES}, root / 'fresh.payload')
            prepared = container_images.stage(root / 'fresh.payload')
            self.assertEqual(prepared.manifest['format'], 2)
            self.assertEqual(receipt['images'], manifest['images'])
            with prepared.archive() as incoming:
                self.assertEqual(lzma.decompress(incoming.read()), raw)
            self.assertTrue(child.stdout.closed)


if __name__ == '__main__':
    unittest.main()
