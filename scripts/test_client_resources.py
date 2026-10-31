"""Large native resources remain bounded and share one exact resource inventory."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from client_resources import resource_map,build_config,inspect_stream
from prepare_client_host import prepare,payload_components
import test_android_runtime
import test_container_images
import test_installer_tools


class ClientResourceTests(unittest.TestCase):
    def setUp(self):
        host=patch('android_runtime.host_key',return_value='macos-arm64');host.start();self.addCleanup(host.stop)

    def test_large_tool_inventory_has_a_bounded_tauri_configuration(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);native=root/'apps/client/src-tauri';(native/'gen/host').mkdir(parents=True)
            mapping={'gen/host/source.payload':'host/source.payload'}
            mapping.update({'gen/host/tools/python/lib/'+str(i)+'-'+('a'*80)+'.py':'host/tools/python/lib/'+str(i)+'-'+('a'*80)+'.py' for i in range(1660)})
            original=dict(mapping)
            with patch('client_resources.resource_map',return_value=mapping):
                config=build_config(root);value=json.loads(config.read_text())['bundle']['resources']
            self.assertLess(config.stat().st_size,8192,'Tauri exports this configuration through one OS environment variable')
            self.assertEqual(value,{'gen/host/source.payload':'host/source.payload','gen/host/tools':'host/tools'})
            self.assertEqual(mapping,original,'The expanded artifact audit inventory must be preserved')
            result=subprocess.check_output([sys.executable,'-c','import os,json;print(json.loads(os.environ["TAURI_CONFIG"])["bundle"]["resources"]["gen/host/tools"])'],
                                           env={**os.environ,'TAURI_CONFIG':config.read_text()},text=True)
            self.assertEqual(result.strip(),'host/tools')

    def test_optional_android_resource_is_bound_and_removed_on_source_only_build(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=test_android_runtime.AndroidRuntimeTests().fixture(root)
            native=root/'apps/client/src-tauri';native.mkdir(parents=True)
            config={'bundle':{'resources':{'gen/host/source.payload':'host/source.payload','gen/host/entry.py':'host/entry.py'}}}
            (native/'tauri.conf.json').write_text(json.dumps(config));(root/'scripts').mkdir();(root/'scripts/client_host_entry.py').write_text('fixture')
            expected=hashlib.sha256(payload.read_bytes()).hexdigest()
            result=prepare(root,android_runtime=payload)
            self.assertEqual(result['android.payload'],expected)
            self.assertEqual(resource_map(root)['gen/host/android.payload'],'host/android.payload')
            self.assertEqual(json.loads(build_config(root).read_text())['bundle']['resources'],resource_map(root))
            self.assertEqual(payload_components(root),['android-runtime'])
            (native/'gen/host/integrity.json').write_text(json.dumps({**result,'private.file':'0'*64}))
            with self.assertRaisesRegex(ValueError,'Unrecognized'):resource_map(root)
            prepare(root);self.assertNotIn('gen/host/android.payload',resource_map(root));self.assertFalse((native/'gen/host/android.payload').exists())

    def test_image_capsule_is_in_the_native_inventory_and_cleared_on_source_build(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=test_container_images.ContainerImageTests().fixture(root)
            native=root/'apps/client/src-tauri';native.mkdir(parents=True)
            (native/'tauri.conf.json').write_text(json.dumps({'bundle':{'resources':{'gen/host/source.payload':'host/source.payload','gen/host/entry.py':'host/entry.py'}}}))
            (root/'scripts').mkdir();(root/'scripts/client_host_entry.py').write_text('fixture')
            result=prepare(root,image_runtime=payload)
            self.assertIn('images.payload',result);self.assertEqual(payload_components(root),['container-images'])
            self.assertEqual(resource_map(root)['gen/host/images.payload'],'host/images.payload')
            prepare(root);self.assertNotIn('gen/host/images.payload',resource_map(root));self.assertFalse((native/'gen/host/images.payload').exists())

    def test_compiled_resource_mismatch_stops_packaging(self):
        from client_resources import verify_compiled_inventory
        with tempfile.TemporaryDirectory() as temp:
            binary=Path(temp)/'fixture';sha='a'*64;binary.write_bytes(b'x'*(1024**2-17)+sha.encode())
            verify_compiled_inventory(binary,{'source.payload':sha})
            with self.assertRaisesRegex(ValueError,'Compiled resources differ'):verify_compiled_inventory(binary,{'source.payload':'b'*64})

    def test_installer_tools_have_one_bound_manifest_and_source_build_omits_the_group(self):
        with tempfile.TemporaryDirectory() as temp,patch('installer_tools.host_key',return_value='macos-arm64'):
            root=Path(temp);fixture=test_installer_tools.InstallerToolTests();archives,pins=fixture.fixture(root);component=root/'tools';fixture._build(archives,component,pins=pins,host='macos-arm64')
            native=root/'apps/client/src-tauri';native.mkdir(parents=True);(native/'tauri.conf.json').write_text(json.dumps({'bundle':{'resources':{'gen/host/source.payload':'host/source.payload','gen/host/entry.py':'host/entry.py'}}}))
            (root/'scripts').mkdir();(root/'scripts/client_host_entry.py').write_text('fixture')
            result=prepare(root,installer_tools=component);mapping=resource_map(root)
            self.assertIn('tools/manifest.json',result);self.assertEqual(payload_components(root),['installer-tools'])
            self.assertEqual(mapping['gen/host/tools/python/bin/python3'],'host/tools/python/bin/python3')
            from client_resources import public_build_digest,file_receipt
            node=native/'gen/host/tools/node/bin/node';sha=hashlib.sha256(node.read_bytes()).hexdigest()
            (root/'config').mkdir();(root/'config/installer-tools.json').write_text(json.dumps({'hosts':{'macos-arm64':{'node':{'public_build_paths_sha256':sha}}}}))
            self.assertEqual(public_build_digest(root,'host/tools/node/bin/node'),sha)
            self.assertIsNone(public_build_digest(root,'host/tools/python/bin/python3'))
            self.assertEqual(file_receipt(node,sha)['sha256'],sha)
            unlisted=native/'gen/host/tools/unlisted';unlisted.write_text('not an approved resource')
            with self.assertRaisesRegex(ValueError,'unlisted'):resource_map(root)
            unlisted.unlink()
            prepare(root);self.assertFalse(any(name.startswith('gen/host/tools/') for name in resource_map(root)))

    def test_expanded_artifact_rejects_unlisted_tool_files(self):
        from client_artifacts import inspect_payload
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);native=root/'apps/client/src-tauri';(native/'gen/host/tools').mkdir(parents=True)
            manifest=native/'gen/host/tools/manifest.json';manifest.write_bytes(b'fixture')
            mapping={'gen/host/tools/manifest.json':'host/tools/manifest.json'}
            files={'usr/lib/ai-phone/host/tools/manifest.json':manifest}
            with patch('client_resources.resource_map',return_value=mapping),patch('client_resources.public_build_digest',return_value=None):
                self.assertEqual(len(inspect_payload(files,root)),1)
                for name in ('usr/lib/ai-phone/host/tools/user.txt','usr/lib/ai-phone/HOST/TOOLS/user.txt'):
                    with self.assertRaisesRegex(ValueError,'Unexpected.*tool'):
                        inspect_payload({**files,name:b'private user note without a key'},root)

    def test_scanning_is_bounded_and_finds_tokens_across_chunks(self):
        class Bounded(io.BytesIO):
            def read(self,size=-1):
                if not 0<size<=1024**2:raise AssertionError('Unbounded resource read')
                return super().read(size)
        data=b'x'*(1024**2-5)+b' safe bytes'
        self.assertEqual(inspect_stream(Bounded(data),'fixture'),{'size':len(data),'sha256':hashlib.sha256(data).hexdigest()})
        for secret in [b' sk-'+b'A'*32,(' sk-'+'A'*32).encode('utf-16-le'),b'/Users/'+b'fixture/private/']:
            with self.assertRaisesRegex(ValueError,'Private material'):inspect_stream(Bounded(b'x'*(1024**2-7)+secret),'fixture')

    def test_public_vendor_build_paths_require_exact_bytes_and_never_exempt_tokens(self):
        data=b'official build path '+b'/Users/'+b'public-builder/source/'
        sha=hashlib.sha256(data).hexdigest()
        with self.assertRaises(ValueError):inspect_stream(io.BytesIO(data),'vendor')
        self.assertEqual(inspect_stream(io.BytesIO(data),'vendor',public_build_digest=sha)['sha256'],sha)
        with self.assertRaises(ValueError):inspect_stream(io.BytesIO(data+b'changed'),'vendor',public_build_digest=sha)
        with self.assertRaises(ValueError):inspect_stream(io.BytesIO(b'changed without a build path'),'vendor',public_build_digest=sha)
        secret=b' sk-'+b'A'*32
        with self.assertRaises(ValueError):inspect_stream(io.BytesIO(secret),'vendor',public_build_digest=hashlib.sha256(secret).hexdigest())

    def test_privacy_scan_keeps_a_bounded_working_window(self):
        from release_package import SECRET
        class Probe:
            def search(self,data):
                if len(data)>1024**2+4096:raise AssertionError('Scanner retained unbounded input')
                return SECRET.search(data)
        data=b'x'*(4*1024**2)
        with patch('release_package.SECRET',Probe()):self.assertEqual(inspect_stream(io.BytesIO(data),'fixture')['sha256'],hashlib.sha256(data).hexdigest())

    def test_compiled_inventory_scan_does_not_retain_the_whole_binary(self):
        import tracemalloc
        from client_resources import verify_compiled_inventory
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'binary';sha='a'*64;path.write_bytes(b'x'*(8*1024**2)+sha.encode())
            tracemalloc.start()
            try:
                verify_compiled_inventory(path,{'source.payload':sha});peak=tracemalloc.get_traced_memory()[1]
            finally:tracemalloc.stop()
            self.assertLess(peak,12*1024**2,'Compiled inventory scan retained the whole binary')


if __name__=='__main__':unittest.main()
