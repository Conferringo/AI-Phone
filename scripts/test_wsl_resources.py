"""Bundled WSL bytes are bound to the native inventory; user state stays separate."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from wsl_backend import publish_capsule
from wsl_resources import prepare, verify
from test_wsl_backend import rootfs

class WslResourceTests(unittest.TestCase):
    def capsule(self,folder):
        image=folder/'input.tar.gz';rootfs(image)
        payload=folder/'backend.payload';publish_capsule(image,payload,{'fixture':True})
        return payload,image

    def test_prepared_component_keeps_exact_rootfs_and_manifest_identity(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,image=self.capsule(root);output=root/'gen';output.mkdir()
            identity=prepare(output,payload)
            self.assertEqual(set(identity),{'wsl/manifest.json'})
            self.assertEqual((output/'wsl/rootfs.tar.gz').read_bytes(),image.read_bytes())
            value=verify(output/'wsl',identity['wsl/manifest.json'])
            self.assertEqual(value['rootfs']['sha256'],hashlib.sha256(image.read_bytes()).hexdigest())
            self.assertFalse(value['windows_boot_verified'])
            self.assertFalse(value['complete_device_bundle'])
            before=(output/'wsl/rootfs.tar.gz').stat().st_mtime_ns
            self.assertEqual(prepare(output,payload),identity)
            self.assertEqual((output/'wsl/rootfs.tar.gz').stat().st_mtime_ns,before)

    def test_bad_capsule_or_changed_copy_cannot_be_admitted(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=self.capsule(root);output=root/'gen';output.mkdir()
            value=prepare(output,payload);(output/'wsl/rootfs.tar.gz').write_bytes(b'changed')
            with self.assertRaises(ValueError):verify(output/'wsl',value['wsl/manifest.json'])
            with self.assertRaises(ValueError):prepare(output,None)
            self.assertEqual((output/'wsl/rootfs.tar.gz').read_bytes(),b'changed')
            with payload.open('r+b') as raw:raw.seek(512);raw.write(b'changed')
            other=root/'other';other.mkdir()
            with self.assertRaises(ValueError):prepare(other,payload)
            self.assertEqual(list(other.iterdir()),[])

    def test_interrupted_copy_leaves_no_partially_published_resource(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=self.capsule(root);output=root/'gen';output.mkdir()
            with patch('wsl_resources.copy_rootfs',side_effect=OSError('fixture write failure')):
                with self.assertRaises(OSError):prepare(output,payload)
            self.assertEqual(list(output.iterdir()),[])

    def test_source_build_clears_only_its_valid_generated_group(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=self.capsule(root);output=root/'gen';output.mkdir()
            user=root/'user/chat';user.parent.mkdir();user.write_text('keep')
            prepare(output,payload);self.assertEqual(prepare(output,None),{})
            self.assertFalse((output/'wsl').exists());self.assertEqual(user.read_text(),'keep')

    def test_native_inventory_and_source_only_rebuild_have_one_consistent_group(self):
        from prepare_client_host import prepare as prepare_client, payload_components
        from client_resources import resource_map,build_config
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=self.capsule(root)
            native=root/'apps/client/src-tauri';native.mkdir(parents=True)
            (native/'tauri.conf.json').write_text(json.dumps({'bundle':{'resources':{'gen/host/source.payload':'host/source.payload'}}}))
            (root/'scripts').mkdir();(root/'scripts/client_host_entry.py').write_text('fixture')
            selected=prepare_client(root,wsl_runtime=payload)
            self.assertIn('wsl/manifest.json',selected)
            self.assertEqual(resource_map(root)['gen/host/wsl/rootfs.tar.gz'],'host/wsl/rootfs.tar.gz')
            self.assertEqual(payload_components(root),['wsl-backend'])
            from client_delivery import delivery_metadata
            delivery=delivery_metadata(payload_components(root),'windows-x64')
            self.assertTrue(delivery['server_included'])
            self.assertFalse(delivery['complete_device_bundle'])
            config=json.loads(build_config(root).read_text())['bundle']['resources']
            self.assertEqual(config['gen/host/wsl'],'host/wsl')
            from client_artifacts import inspect_payload
            files={destination:native/source for source,destination in resource_map(root).items()}
            inspect_payload(files,root)
            files['host/wsl/unbound.dat']=b'unbound fixture'
            with self.assertRaisesRegex(ValueError,'Unexpected.*resource'):
                inspect_payload(files,root)
            prepare_client(root)
            self.assertNotIn('gen/host/wsl/rootfs.tar.gz',resource_map(root))
            self.assertFalse((native/'gen/host/wsl').exists())

if __name__=='__main__':unittest.main()
