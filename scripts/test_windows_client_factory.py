"""Complete Windows payload and same-source backend handoff contracts."""
import json
from contextlib import chdir
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from artifact_io import digest
import windows_client_factory as factory
from test_wsl_backend import rootfs
from wsl_backend import publish_capsule


class WindowsClientFactoryTests(unittest.TestCase):
    def fixture(self, root):
        (root/'dist').mkdir()
        (root/'SOURCE-MANIFEST.json').write_text('same source')
        archive = root/'backend.tar.gz'
        rootfs(archive)
        payload = root/'backend.payload'
        component = publish_capsule(archive, payload, {'source_components': sorted(factory.BACKEND)})
        report = {'source_commit': 'a'*40, 'passed': True, 'component': component,
                  'runtime': {'passed': True, 'phases': [{'checks': {'frame': True}}, {'checks': {'frame': True}}],
                              'cleanup': {'container': True, 'image': True, 'volume': True}}}
        return payload, report

    def test_complete_windows_inventory_uses_its_native_android_and_owned_backend(self):
        value = factory.checked_delivery(['android-runtime', 'wsl-backend'])
        self.assertTrue(value['complete_device_bundle'])
        self.assertTrue(value['server_included'])
        self.assertNotIn('windows_boot_verified', value)
        for incomplete in ([], ['wsl-backend'], ['android-runtime']):
            with self.subTest(incomplete=incomplete), self.assertRaises(ValueError):
                factory.checked_delivery(incomplete)

    def test_same_source_handoff_retains_real_capsule_and_rejects_changed_bytes(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {'GITHUB_SHA': 'a'*40}):
            root = Path(temporary)
            payload, report = self.fixture(root)
            path = factory.stage_backend(root, report, payload)
            identity = digest(path)
            admitted, component = factory.admit_backend(root, path.parent, identity)
            self.assertEqual(admitted.read_bytes(), payload.read_bytes())
            self.assertEqual(component, report['component'])
            with self.assertRaises(FileExistsError):factory.stage_backend(root, report, payload)
            admitted.write_bytes(b'changed capsule')
            with self.assertRaisesRegex(ValueError, 'bytes changed'):
                factory.admit_backend(root, path.parent, identity)

    def test_relative_backend_argument_admits_the_same_qualified_payload(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {'GITHUB_SHA': 'a'*40}):
            root = Path(temporary).resolve()
            payload, report = self.fixture(root)
            path = factory.stage_backend(root, report, payload)
            identity = digest(path)
            with chdir(root):
                admitted, component = factory.admit_backend(root, Path('dist/windows-backend'), identity)
                self.assertEqual(admitted.read_bytes(), payload.read_bytes())
                self.assertEqual(component, report['component'])

    def test_component_only_false_qualification_and_foreign_source_never_stage(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {'GITHUB_SHA': 'a'*40}):
            root = Path(temporary)
            payload, report = self.fixture(root)
            for change in ('source', 'apps', 'passed', 'phase', 'cleanup'):
                current = json.loads(json.dumps(report))
                if change == 'source':current['source_commit'] = 'b'*40
                elif change == 'apps':current['component']['provenance']['source_components'].remove('phone-apps')
                elif change == 'passed':current['passed'] = False
                elif change == 'phase':current['runtime']['phases'][1]['checks']['frame'] = False
                else:current['runtime']['cleanup']['volume'] = False
                with self.subTest(change=change), self.assertRaises(ValueError):
                    factory.stage_backend(root, current, payload)
                self.assertFalse((root/'dist/windows-backend').exists())

    def test_backend_handoff_refuses_metadata_source_inventory_and_extra_files(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {'GITHUB_SHA': 'a'*40}):
            root = Path(temporary)
            payload, report = self.fixture(root)
            path = factory.stage_backend(root, report, payload)
            identity = digest(path)
            with patch.dict(os.environ, {'GITHUB_SHA': 'b'*40}):
                with self.assertRaisesRegex(ValueError, 'source'):factory.admit_backend(root, path.parent, identity)
            (root/'SOURCE-MANIFEST.json').write_text('other source')
            with self.assertRaisesRegex(ValueError, 'inventory'):factory.admit_backend(root, path.parent, identity)
            (path.parent/'unbound').write_text('unexpected')
            with self.assertRaisesRegex(ValueError, 'inventory'):factory.admit_backend(root, path.parent, identity)

    def test_native_compile_embeds_both_capsules_without_rebuilding_the_linux_server(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary);(root/'config').mkdir();(root/'dist').mkdir()
            (root/'config/release-policy.json').write_text('{"version":"fixture"}')
            artifact = root/'dist/AI-Phone-fixture-windows-x64-setup.exe';artifact.write_bytes(b'inspected fixture')
            manifest = {'prebuilt_components': ['android-runtime', 'wsl-backend'], 'complete_device_bundle': True, 'sha256': digest(artifact)}
            receipt = artifact.with_suffix('.exe.manifest.json');receipt.write_text(json.dumps(manifest))
            with patch.object(factory, 'run') as run, patch('prepare_client_host.payload_components', return_value=manifest['prebuilt_components']):
                result = factory.compile_client(root, root/'android.payload', root/'backend.payload')
                self.assertEqual(result['sha256'], digest(artifact))
                command = run.call_args.args[0]
                for flag, expected in [('--platform', 'windows'), ('--android-runtime', root/'android.payload'), ('--wsl-runtime', root/'backend.payload')]:
                    self.assertEqual(command[command.index(flag)+1], expected)
                self.assertNotIn('--with-web-runtime', command)
                manifest['sha256'] = '0'*64;receipt.write_text(json.dumps(manifest))
                with self.assertRaisesRegex(ValueError, 'changed'):factory.compile_client(root, root/'android.payload', root/'backend.payload')


if __name__ == '__main__':unittest.main()
