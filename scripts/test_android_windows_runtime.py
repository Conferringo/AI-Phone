"""Windows Android resource admission without enabling an unfinished host installer."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tarfile
import unittest
import zipfile
from unittest.mock import patch

from android_runtime import checked_manifest,stage,verify_sdk
from client_delivery import delivery_metadata


class WindowsAndroidRuntimeTests(unittest.TestCase):
    def test_factory_builds_the_matched_windows_capsule_without_user_state(self):
        from android_component_factory import build
        from artifact_io import digest
        with tempfile.TemporaryDirectory() as temporary,patch('android_runtime.host_key',return_value='windows-x64'),patch('android_component_factory.host_key',return_value='windows-x64'),patch('android_runtime.shutil.disk_usage',return_value=type('Usage',(),{'free':16*1024**3})()):
            root=Path(temporary);(root/'config').mkdir();pins={};image=root/'image.zip'
            with zipfile.ZipFile(image,'w') as target:
                for name in ('system.img','vendor.img','kernel-ranchu','ramdisk.img','NOTICE.txt'):target.writestr('x86_64/'+name,b'factory')
            for role,program in [('emulator','emulator.exe'),('platform-tools','adb.exe')]:
                source=root/(role+'.zip')
                with zipfile.ZipFile(source,'w') as target:
                    target.writestr(role+'/'+program,b'fixture');target.writestr(role+'/source.properties','Pkg.Revision = 37.0.1\n')
                pins[role]={'url':source.as_uri(),'sha256':digest(source),'size_bytes':source.stat().st_size,'version':'37.0.1','channel':'stable'}
            config={'format':1,'api':35,'license_id':'android-sdk-license','hosts':{'windows-x64':{'abi':'x86_64','archives':pins}},
                    'images':{'x86_64':{'url':image.as_uri(),'sha256':digest(image),'size_bytes':image.stat().st_size}}}
            (root/'config/android-factory.json').write_text(json.dumps(config));(root/'user-chat').write_text('keep')
            output,result=build(root,root/'work');self.assertEqual(result['host'],'windows-x64')
            sdk=stage(output,root/'install');self.assertEqual((sdk/'platform-tools/adb.exe').read_bytes(),b'fixture')
            self.assertEqual((root/'user-chat').read_text(),'keep');self.assertEqual(list((root/'work').glob('.sdk-*')),[])
            with self.assertRaises(FileExistsError):build(root,root/'work')
            config['hosts']['windows-x64']['archives']['emulator']['channel']='preview'
            (root/'config/android-factory.json').write_text(json.dumps(config))
            with self.assertRaisesRegex(ValueError,'stable'):build(root,root/'invalid')
            self.assertFalse((root/'invalid').exists())

    @unittest.skipUnless(os.name=='nt','NTFS directory-junction admission requires Windows')
    def test_sdk_component_junction_never_imports_an_outside_tree(self):
        from android_runtime import tools_inventory
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);sdk=root/'sdk';sdk.mkdir();outside=root/'outside';outside.mkdir()
            (outside/'source.properties').write_text('fixture');(outside/'private').write_text('keep')
            subprocess.run(['cmd','/c','mklink','/J',str(sdk/'emulator'),str(outside)],check=True,capture_output=True)
            with self.assertRaisesRegex(ValueError,'link'):tools_inventory(sdk,'x86_64')
            self.assertEqual((outside/'private').read_text(),'keep')

    def manifest(self):
        base='sdk/system-images/android-35/default/x86_64/'
        names=['sdk/emulator/emulator.exe','sdk/platform-tools/adb.exe',*[base+name for name in ('system.img','vendor.img','kernel-ranchu','ramdisk.img','NOTICE.txt')]]
        return {'format':1,'host':'windows-x64','abi':'x86_64','api':35,'redistribution':'review-pending',
                'files':[{'path':name,'size':7,'sha256':hashlib.sha256(b'fixture').hexdigest(),'mode':0o755 if name.endswith('.exe') else 0o644} for name in names]}

    def test_windows_capsule_is_staged_verified_and_never_promotes_the_host(self):
        with tempfile.TemporaryDirectory() as temporary,patch('android_runtime.host_key',return_value='windows-x64'),patch('android_runtime.shutil.disk_usage',return_value=type('Usage',(),{'free':16*1024**3})()):
            root=Path(temporary);value=self.manifest();payload=root/'android.payload'
            with tarfile.open(payload,'w:gz') as target:
                for name,data in [('ANDROID-MANIFEST.json',json.dumps(value).encode()),*[(row['path'],b'fixture') for row in value['files']]]:
                    entry=tarfile.TarInfo(name);entry.size=len(data);target.addfile(entry,io.BytesIO(data))
            sdk=stage(payload,root/'install');self.assertEqual(verify_sdk(sdk)['host'],'windows-x64')
            self.assertEqual(stage(payload,root/'install'),sdk)
            (sdk/'platform-tools/adb.exe').write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'integrity'):verify_sdk(sdk)
        self.assertFalse(delivery_metadata(['android-runtime'],'windows-x64')['complete_device_bundle'])

    def test_host_names_case_aliases_and_ntfs_streams_cannot_be_admitted(self):
        with patch('android_runtime.host_key',return_value='windows-x64'):
            value=self.manifest();self.assertEqual(checked_manifest(value)['abi'],'x86_64')
            wrong_program={**value,'files':[{**value['files'][0],'path':'sdk/emulator/emulator'},*value['files'][1:]]}
            with self.assertRaisesRegex(ValueError,'Incomplete'):checked_manifest(wrong_program)
            for name in ['sdk/emulator/CON','sdk/emulator/aux.txt','sdk/emulator/file:stream','sdk/emulator/end.','sdk/emulator/end ']:
                changed={**value,'files':[*value['files'],{**value['files'][0],'path':name}]}
                with self.subTest(name=name),self.assertRaisesRegex(ValueError,'path'):checked_manifest(changed)
            duplicate={**value['files'][0],'path':'sdk/emulator/EMULATOR.exe'}
            with self.assertRaises(ValueError):checked_manifest({**value,'files':[*value['files'],duplicate]})
        with patch('android_runtime.host_key',return_value='linux-x64'):
            with self.assertRaisesRegex(ValueError,'host'):checked_manifest(value)


if __name__=='__main__':unittest.main()
