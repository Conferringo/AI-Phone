"""Exercise actual disk-image contents; other build hosts explicitly skip this test."""
import os
import platform
import plistlib
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from macos_disk_image import build

@unittest.skipUnless(platform.system()=='Darwin','macOS hdiutil required')
class MacOSDiskImageTests(unittest.TestCase):
    def test_transient_busy_creation_retries_once_then_builds_a_real_verified_image(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';(app/'Contents').mkdir(parents=True)
            (app/'Contents/runtime.payload').write_bytes(b'immutable runtime fixture')
            output=root/'client.dmg';native_run=subprocess.run;creates=[]
            def native(args,**kwargs):
                if args[1]=='create':
                    creates.append(args)
                    if len(creates)==1:raise subprocess.CalledProcessError(1,args,stderr='hdiutil: create failed - Resource busy')
                return native_run(args,**kwargs)
            with patch('macos_disk_image.subprocess.run',side_effect=native):build(app,output)
            self.assertEqual(len(creates),2)
            details=plistlib.loads(subprocess.check_output(['hdiutil','imageinfo','-plist',str(output)]))
            self.assertEqual(details['Format'],'ULMO')
            self.assertEqual((app/'Contents/runtime.payload').read_bytes(),b'immutable runtime fixture')

    def test_verified_image_contains_the_complete_app_and_applications_link(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';(app/'Contents').mkdir(parents=True)
            payload=b'complete reviewed runtime\x00'*1024
            (app/'Contents/runtime.payload').write_bytes(payload)
            output=root/'client.dmg';build(app,output)
            details=plistlib.loads(subprocess.check_output(['hdiutil','imageinfo','-plist',str(output)]))
            self.assertEqual(details['Format'],'ULMO')
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

    def test_native_verification_error_is_visible_and_never_promotes_a_corrupt_image(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';app.mkdir()
            output=root/'client.dmg';output.write_bytes(b'previous')
            native_run=subprocess.run
            def command(args,**kwargs):
                if args[1]=='create':
                    Path(args[-1]).write_bytes(b'corrupt image')
                    return subprocess.CompletedProcess(args,0,stdout='',stderr='')
                return native_run(args,**kwargs)
            with patch('macos_disk_image.subprocess.run',side_effect=command):
                with self.assertRaises(subprocess.CalledProcessError) as raised:build(app,output)
            diagnostic=(raised.exception.stderr or raised.exception.stdout or '').strip()
            self.assertTrue(diagnostic,'the real hdiutil verification error must be captured')
            self.assertIn(diagnostic,'\n'.join(getattr(raised.exception,'__notes__',[])))
            self.assertEqual(output.read_bytes(),b'previous')

class DiskImageFailureDiagnosticsTests(unittest.TestCase):
    def test_busy_retry_is_bounded_and_preserves_the_existing_installer(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';app.mkdir();output=root/'client.dmg';output.write_bytes(b'previous')
            def busy(args,**_kwargs):raise subprocess.CalledProcessError(1,args,stderr='hdiutil: create failed - Resource busy')
            with patch('macos_disk_image.subprocess.run',side_effect=busy) as run:
                with self.assertRaises(subprocess.CalledProcessError):build(app,output)
                self.assertEqual(run.call_count,2)
            self.assertEqual(output.read_bytes(),b'previous')

    def test_partial_busy_creation_and_other_errors_are_not_replayed(self):
        for partial,code,diagnostic in [(True,1,'hdiutil: create failed - Resource busy'),
                                        (False,1,'hdiutil: create failed - Permission denied'),
                                        (False,7,'hdiutil: create failed - Resource busy')]:
            with self.subTest(partial=partial,code=code,diagnostic=diagnostic),tempfile.TemporaryDirectory() as folder:
                root=Path(folder);app=root/'AI Phone.app';app.mkdir();output=root/'client.dmg';output.write_bytes(b'previous')
                def fail(args,**_kwargs):
                    if partial:Path(args[-1]).write_bytes(b'partial image')
                    raise subprocess.CalledProcessError(code,args,stderr=diagnostic)
                with patch('macos_disk_image.subprocess.run',side_effect=fail) as run:
                    with self.assertRaises(subprocess.CalledProcessError):build(app,output)
                    self.assertEqual(run.call_count,1)
                self.assertEqual(output.read_bytes(),b'previous')

    def test_creation_failure_explains_native_cause_without_replacing_existing_image(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';app.mkdir()
            output=root/'client.dmg';output.write_bytes(b'previous')
            error=subprocess.CalledProcessError(1,['hdiutil','create'],stderr='hdiutil: create failed - Resource busy')
            with patch('macos_disk_image.subprocess.run',side_effect=error):
                with self.assertRaises(subprocess.CalledProcessError) as raised:build(app,output)
            notes='\n'.join(getattr(raised.exception,'__notes__',[]))
            self.assertIn('DMG create failed',notes)
            self.assertIn('Resource busy',notes)
            self.assertEqual(output.read_bytes(),b'previous')
            self.assertEqual({p.name for p in root.iterdir()},{'AI Phone.app','client.dmg'})

    def test_verification_failure_reports_checksum_error_and_preserves_existing_image(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';app.mkdir()
            output=root/'client.dmg';output.write_bytes(b'previous')
            def native(args,**kwargs):
                if args[1]=='create':
                    Path(args[-1]).write_bytes(b'unverified image')
                    return subprocess.CompletedProcess(args,0,stdout='',stderr='')
                raise subprocess.CalledProcessError(1,args,output='checksum mismatch')
            with patch('macos_disk_image.subprocess.run',side_effect=native):
                with self.assertRaises(subprocess.CalledProcessError) as raised:build(app,output)
            notes='\n'.join(getattr(raised.exception,'__notes__',[]))
            self.assertIn('DMG verify failed',notes)
            self.assertIn('checksum mismatch',notes)
            self.assertEqual(output.read_bytes(),b'previous')
            self.assertEqual({p.name for p in root.iterdir()},{'AI Phone.app','client.dmg'})

    def test_silent_native_failure_retains_exit_code_and_reports_missing_diagnostics(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);app=root/'AI Phone.app';app.mkdir()
            with patch('macos_disk_image.subprocess.run',side_effect=subprocess.CalledProcessError(7,['hdiutil','create'])):
                with self.assertRaises(subprocess.CalledProcessError) as raised:build(app,root/'client.dmg')
            self.assertEqual(raised.exception.returncode,7)
            notes='\n'.join(getattr(raised.exception,'__notes__',[]))
            self.assertIn('exit 7',notes)
            self.assertIn('no native diagnostic output',notes)

if __name__=='__main__':unittest.main()
