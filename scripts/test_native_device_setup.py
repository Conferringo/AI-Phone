"""Closed native setup packaging/caller contracts; no executable or device is run."""
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch
import build_native_services as builder
import prebuilt_services as services
import prebuilt_js
import test_native_services


class NativeDeviceSetupTests(unittest.TestCase):
    def test_actual_capability_probe_accepts_only_exact_success(self):
        path=Path('/fixture/assistant')
        with patch.object(builder.subprocess,'run',return_value=SimpleNamespace(returncode=0,stdout=services.DEVICE_SETUP_CONTRACT+'\n',stderr='')) as run:
            self.assertEqual(builder.device_setup_receipt(path),services.DEVICE_SETUP_CONTRACT)
            self.assertEqual(run.call_args.args[0],[str(path),'--device-setup-contract'])
            self.assertEqual(run.call_args.kwargs['timeout'],10)
        for code,stdout,stderr in [(1,services.DEVICE_SETUP_CONTRACT,''),(0,'unrelated',''),(0,services.DEVICE_SETUP_CONTRACT,'fixture-error')]:
            with patch.object(builder.subprocess,'run',return_value=SimpleNamespace(returncode=code,stdout=stdout,stderr=stderr)):
                with self.assertRaises(ValueError):builder.device_setup_receipt(path)

    def test_verified_component_dispatches_existing_host_matched_binary(self):
        helper=test_native_services.NativeServiceTests()
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder).resolve();files=helper.component()
            manifest=json.loads(files['runtime/services/manifest.json']);manifest['device_setup']=services.DEVICE_SETUP_CONTRACT
            files['runtime/services/manifest.json']=json.dumps(manifest).encode()
            for name,data in files.items():
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
                path.chmod(0o755 if name.startswith('runtime/services/bin/') else 0o644)
            with patch.object(builder.subprocess,'run',side_effect=AssertionError('No process allowed for prebuilt selection')):
                spec=services.device_setup_spec(root)
                self.assertEqual(spec,{'argv':[str(root/'runtime/services/bin/assistant'),'--setup-devices'],'prebuilt':True})
                manifest.pop('device_setup');(root/'runtime/services/manifest.json').write_text(json.dumps(manifest))
                with self.assertRaisesRegex(ValueError,'capability'):services.device_setup_spec(root)

    def test_source_dispatch_requires_actual_readonly_capability(self):
        with tempfile.TemporaryDirectory() as folder,patch.object(builder,'device_setup_receipt') as qualify:
            root=Path(folder).resolve();spec=services.device_setup_spec(root)
            qualify.assert_called_once_with(root/'target/debug/ai-phone-assistant')
            self.assertEqual(spec['argv'],[str(root/'target/debug/ai-phone-assistant'),'--setup-devices'])
        with tempfile.TemporaryDirectory() as folder,patch.object(builder,'device_setup_receipt',side_effect=ValueError('old binary')):
            with self.assertRaises(ValueError):services.device_setup_spec(folder)

    def test_legacy_device_setup_is_not_a_js_entry(self):
        self.assertNotIn('device-setup',prebuilt_js.ENTRIES)
        self.assertNotIn('scripts/client-device-setup.ts',prebuilt_js.ENTRIES.values())
        with self.assertRaises(ValueError):prebuilt_js.launch_spec(Path('/unused'),'device-setup')

    def test_native_source_identity_includes_actual_caller_and_selector(self):
        rows=[('scripts/client_host.py',b'caller',0o644),('scripts/prebuilt_services.py',b'selector',0o644),('crates/assistant/src/device_setup/mod.rs',b'owner',0o644)]
        with patch.object(builder,'select_files',return_value=rows):baseline=builder.source_fingerprint(Path('/unused'))
        for index in range(len(rows)):
            changed=list(rows);name,data,mode=changed[index];changed[index]=(name,data+b' changed',mode)
            with patch.object(builder,'select_files',return_value=changed):self.assertNotEqual(builder.source_fingerprint(Path('/unused')),baseline)

    def test_old_or_malformed_build_capability_is_not_reused(self):
        helper=test_native_services.NativeServiceTests()
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder).resolve();receipt=helper.build_fixture(root);path=root/'target/release/native-service-build.json'
            self.assertIsNotNone(builder.read_build_receipt(path))
            for checks in [None,[],{}, {'device_setup':'wrong'}]:
                receipt['checks']=checks;path.write_text(json.dumps(receipt));self.assertIsNone(builder.read_build_receipt(path))
