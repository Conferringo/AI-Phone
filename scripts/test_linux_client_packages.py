"""One native compilation produces both independently inspected Linux formats."""
from pathlib import Path
import json
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import build_client as client


class LinuxClientPackageTests(unittest.TestCase):
    def fixture(self,root,arch='amd64'):
        for kind,extension,label in [('deb','deb',arch),('appimage','AppImage','aarch64' if arch=='arm64' else arch)]:
            source=root/f'apps/client/src-tauri/target/release/bundle/{kind}/AI Phone_fixture_{label}.{extension}'
            source.parent.mkdir(parents=True);source.write_bytes(kind.encode());source.chmod(0o755)
        (root/'dist').mkdir()

    def test_compile_once_and_inspect_both_formats_before_reporting(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.fixture(root);command=['node','owned-tauri.js'];env={'CI':'true'}
            with patch.object(client.platform,'system',return_value='Linux'),patch.object(client.platform,'machine',return_value='x86_64'),\
                 patch.object(client.subprocess,'run') as run,patch.object(client.shutil,'which',return_value='/usr/bin/patchelf'),patch.object(client,'inspect_deb',return_value={'deb':'inspected'}) as deb,\
                 patch.object(client,'inspect_appimage',return_value={'appimage':'inspected'}) as appimage,\
                 patch.object(client,'record',return_value={'server_included':True,'complete_device_bundle':True}) as record,\
                 patch.object(client,'compact_appimage') as compact:
                value=client.build_linux(root,'fixture',env,command,False)
                self.assertEqual(run.call_count,1)
                arguments=run.call_args.args[0];self.assertEqual(arguments[arguments.index('--bundles')+1],'deb,appimage')
                self.assertEqual(run.call_args.kwargs['env']['CI'],env['CI'])
                self.assertEqual(run.call_args.kwargs['env']['AI_PHONE_REAL_PATCHELF'],'/usr/bin/patchelf')
                self.assertEqual(run.call_args.kwargs['env']['LDAI_COMP'],'zstd')
                self.assertIn(json.dumps({'bundle':{'useLocalToolsDir':True}},separators=(',',':')),arguments)
                folder=root/'apps/client/src-tauri/target/release/bundle/appimage'
                compact.assert_called_once_with(root,folder/'AI Phone.AppDir',folder/'AI Phone_fixture_amd64.AppImage','amd64',run.call_args.kwargs['env'])
                self.assertEqual(len(value),2);self.assertEqual(record.call_count,2)
                deb.assert_called_once_with(root/'dist/AI-Phone-fixture-linux-amd64.deb',root)
                appimage.assert_called_once_with(root/'dist/AI-Phone-fixture-linux-amd64.AppImage','amd64',root)
                self.assertEqual({Path(row['artifact']).suffix for row in value},{'.deb','.AppImage'})
                self.assertTrue(all(row['complete_device_bundle'] for row in value))
                for name,body in [('deb',b'deb'),('AppImage',b'appimage')]:
                    self.assertEqual((root/f'dist/AI-Phone-fixture-linux-amd64.{name}').read_bytes(),body)

    def test_failed_repack_preserves_the_built_image_and_stops_the_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source=root/'built.AppImage';source.write_bytes(b'original built image')
            with patch.object(client.subprocess,'check_output',return_value='8\n'),\
                 patch.object(client.subprocess,'run',side_effect=[None,subprocess.CalledProcessError(1,'appimagetool')]),\
                 self.assertRaises(subprocess.CalledProcessError):
                client.compact_appimage(root,root/'AppDir',source,'amd64',{})
            self.assertEqual(source.read_bytes(),b'original built image')

    def test_existing_arm_packages_do_not_recompile_and_keep_the_appimage_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.fixture(root,'arm64')
            with patch.object(client.platform,'system',return_value='Linux'),patch.object(client.platform,'machine',return_value='aarch64'),\
                 patch.object(client.subprocess,'run') as run,patch.object(client,'inspect_deb',return_value={}),\
                 patch.object(client,'inspect_appimage',return_value={}),\
                 patch.object(client,'record',return_value={'server_included':False,'complete_device_bundle':False}):
                value=client.build_linux(root,'fixture',{},['node','owned-tauri.js'],True)
                run.assert_not_called();self.assertEqual(len(value),2)
                self.assertEqual((root/'dist/AI-Phone-fixture-linux-arm64.AppImage').stat().st_mode&0o111,0o111)


if __name__=='__main__':unittest.main()
