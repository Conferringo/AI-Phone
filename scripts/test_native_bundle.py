import json
from pathlib import Path
import plistlib
import tempfile
import unittest
from native_bundle import inspect_macos_bundle


class NativeBundleTests(unittest.TestCase):
    def test_bundle_accepts_matching_resources_and_rejects_state_tampering_and_stale_legal_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder); bundle=root/'fixture.app'
            for path in ['config','apps/client/src-tauri','fixture.app/Contents/MacOS','fixture.app/Contents/Resources/legal']:
                (root/path).mkdir(parents=True,exist_ok=True)
            (root/'config/release-policy.json').write_text(json.dumps({'version':'0.1.0'}))
            (root/'LICENSE').write_text('fixture legal text')
            (root/'apps/client/src-tauri/tauri.conf.json').write_text(json.dumps({'version':'0.1.0','identifier':'dev.aiphone.client',
                'bundle':{'macOS':{'minimumSystemVersion':'14.4'},'resources':{'../../../LICENSE':'legal/LICENSE'}}}))
            (bundle/'Contents/Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'dev.aiphone.client','CFBundleShortVersionString':'0.1.0',
                'CFBundleExecutable':'client','LSMinimumSystemVersion':'14.4'}))
            (bundle/'Contents/MacOS/client').write_bytes(b'fixture binary')
            (bundle/'Contents/Resources/icon.icns').write_bytes(b'fixture icon')
            legal=bundle/'Contents/Resources/legal/LICENSE';legal.write_text('fixture legal text')
            self.assertEqual(len(inspect_macos_bundle(root,bundle)['files']),4)
            private=bundle/'Contents/Resources/server.json';private.write_text('private state')
            with self.assertRaisesRegex(ValueError,'Unexpected bundle file'):inspect_macos_bundle(root,bundle)
            private.unlink();legal.write_text('stale')
            with self.assertRaisesRegex(ValueError,'Stale/missing'):inspect_macos_bundle(root,bundle)
            legal.write_text('fixture legal text')
            (bundle/'Contents/MacOS/client').write_bytes(('/Users/'+'fixture/private/').encode())
            with self.assertRaisesRegex(ValueError,'Private material'):inspect_macos_bundle(root,bundle)


if __name__=='__main__':unittest.main()
