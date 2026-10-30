"""Large native resources remain bounded and share one exact resource inventory."""
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from client_resources import resource_map,build_config,inspect_stream
from prepare_client_host import prepare,payload_components
import test_android_runtime


class ClientResourceTests(unittest.TestCase):
    def setUp(self):
        host=patch('android_runtime.host_key',return_value='macos-arm64');host.start();self.addCleanup(host.stop)

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

    def test_compiled_resource_mismatch_stops_packaging(self):
        from client_resources import verify_compiled_inventory
        with tempfile.TemporaryDirectory() as temp:
            binary=Path(temp)/'fixture';sha='a'*64;binary.write_bytes(b'x'*(1024**2-17)+sha.encode())
            verify_compiled_inventory(binary,{'source.payload':sha})
            with self.assertRaisesRegex(ValueError,'Compiled resources differ'):verify_compiled_inventory(binary,{'source.payload':'b'*64})

    def test_scanning_is_bounded_and_finds_tokens_across_chunks(self):
        class Bounded(io.BytesIO):
            def read(self,size=-1):
                if not 0<size<=1024**2:raise AssertionError('Unbounded resource read')
                return super().read(size)
        data=b'x'*(1024**2-5)+b' safe bytes'
        self.assertEqual(inspect_stream(Bounded(data),'fixture'),{'size':len(data),'sha256':hashlib.sha256(data).hexdigest()})
        for secret in [b' sk-'+b'A'*32,(' sk-'+'A'*32).encode('utf-16-le'),b'/Users/'+b'fixture/private/']:
            with self.assertRaisesRegex(ValueError,'Private material'):inspect_stream(Bounded(b'x'*(1024**2-7)+secret),'fixture')


if __name__=='__main__':unittest.main()
