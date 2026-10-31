"""Exercise actual disk-image contents; other build hosts explicitly skip this test."""
import os
import platform
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from macos_disk_image import build

@unittest.skipUnless(platform.system()=='Darwin','macOS hdiutil required')
class MacOSDiskImageTests(unittest.TestCase):
    def test_verified_image_contains_the_complete_app_and_applications_link(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';(app/'Contents').mkdir(parents=True)
            payload=b'complete reviewed runtime\x00'*1024
            (app/'Contents/runtime.payload').write_bytes(payload)
            output=root/'client.dmg';build(app,output)
            mount=root/'mounted';mount.mkdir()
            subprocess.run(['hdiutil','attach','-quiet','-readonly','-nobrowse','-mountpoint',str(mount),str(output)],check=True)
            try:
                self.assertEqual((mount/'AI Phone.app/Contents/runtime.payload').read_bytes(),payload)
                self.assertEqual(os.readlink(mount/'Applications'),'/Applications')
                self.assertEqual({p.name for p in mount.iterdir() if not p.name.startswith('.')},{'AI Phone.app','Applications'})
                self.assertEqual((app/'Contents/runtime.payload').read_bytes(),payload)
            finally:
                subprocess.run(['hdiutil','detach','-quiet',str(mount)],check=True)

    def test_failed_image_creation_preserves_the_previous_artifact(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';app.mkdir();output=root/'client.dmg';output.write_bytes(b'previous')
            with patch('macos_disk_image.subprocess.run',side_effect=subprocess.CalledProcessError(1,'hdiutil')):
                with self.assertRaises(subprocess.CalledProcessError):build(app,output)
            self.assertEqual(output.read_bytes(),b'previous')
            self.assertEqual({p.name for p in root.iterdir()},{'AI Phone.app','client.dmg'})

if __name__=='__main__':unittest.main()
