"""Full Linux factory contracts reject incomplete plans and unsafe SDK archives."""
import os
import shutil
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest.mock import patch
from artifact_io import digest

import linux_client_factory as factory
import android_sdk_archive as sdk_archive


class LinuxClientFactoryTests(unittest.TestCase):
    @unittest.skipUnless(os.name=='posix','Complete Linux factory uses its POSIX Bun entry')
    def test_nested_checks_find_project_bun_without_a_global_install(self):
        with tempfile.TemporaryDirectory(prefix='factory path ') as temporary:
            root=Path(temporary);(root/'scripts').mkdir();(root/'.tools/bin').mkdir(parents=True)
            shutil.copyfile(Path(__file__).with_name('bun'),root/'scripts/bun');(root/'scripts/bun').chmod(0o755)
            program=root/'.tools/bin/bun'
            program.write_text('#!/bin/sh\nif [ "$1" = run ]; then exec bun --version; fi\necho fixture-nested-bun\n')
            program.chmod(0o755)
            with patch.dict(os.environ,{'PATH':'/usr/bin:/bin'},clear=False):
                result=factory.run_checks(root,capture_output=True,text=True)
            self.assertEqual(result.stdout.strip(),'fixture-nested-bun')

    def test_pinned_factory_download_rejects_bad_size_and_invalid_destination_name(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'input';source.write_bytes(b'verified')
            pin={'url':source.as_uri(),'sha256':digest(source),'size_bytes':8}
            destination=factory.fetch(root/'cache','input',pin)
            self.assertEqual(destination.read_bytes(),b'verified')
            with self.assertRaisesRegex(ValueError,'size'):factory.fetch(root/'cache','input',{**pin,'size_bytes':9})
            with self.assertRaisesRegex(ValueError,'name'):factory.fetch(root/'cache','../outside',pin)
            self.assertFalse((root/'outside').exists())

    def test_factory_refuses_old_or_classic_docker_before_building_any_image(self):
        modern={'Client':{'ApiVersion':'1.52'},'Server':{'ApiVersion':'1.52'}}
        store={'DriverStatus':[['driver-type','io.containerd.snapshotter.v1']]}
        def probe(version=modern,info=store):
            import json
            return patch('linux_client_factory.subprocess.check_output',side_effect=[json.dumps(version),json.dumps(info)])
        with patch('linux_client_factory.subprocess.run'):
            with probe():factory.check_docker()
            for version,info in [({**modern,'Client':{'ApiVersion':'1.48'}},store),({**modern,'Server':{'ApiVersion':'1.48'}},store),(modern,{'DriverStatus':[['Backing Filesystem','extfs']]})]:
                with probe(version,info),self.assertRaisesRegex(ValueError,'Docker'):factory.check_docker()

    def test_factory_refuses_missing_buildx_before_inspecting_other_inputs(self):
        import json,subprocess
        version=json.dumps({'Client':{'ApiVersion':'1.52'},'Server':{'ApiVersion':'1.52'}})
        store=json.dumps({'DriverStatus':[['driver-type','io.containerd.snapshotter.v1']]})
        for error in (FileNotFoundError('docker missing'),subprocess.CalledProcessError(1,['docker','buildx','version'])):
            with patch('linux_client_factory.subprocess.run',side_effect=error) as run,patch('linux_client_factory.subprocess.check_output',side_effect=[version,store]) as inspect:
                with self.assertRaisesRegex(ValueError,'Buildx'):factory.check_docker()
                run.assert_called_once_with(['docker','buildx','version'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                inspect.assert_not_called()

    def test_sdk_directory_metadata_and_exact_file_budget(self):
        entry=zipfile.ZipInfo('emulator/');entry.external_attr=0o040755<<16
        self.assertEqual(sdk_archive.sdk_name(entry,'emulator'),'emulator')
        entry.filename='emulator/bad-directory'
        with self.assertRaisesRegex(ValueError,'directory'):sdk_archive.sdk_name(entry,'emulator')
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'input.zip'
            with zipfile.ZipFile(path,'w') as output:output.writestr('emulator/bin',b'123')
            with zipfile.ZipFile(path) as source,patch('android_sdk_archive.SDK_MAX_FILES',1),patch('android_sdk_archive.SDK_MAX_BYTES',3):
                self.assertEqual(len(list(sdk_archive.sdk_members(source,'emulator'))),1)
            with zipfile.ZipFile(path) as source,patch('android_sdk_archive.SDK_MAX_FILES',0):
                with self.assertRaisesRegex(ValueError,'budget'):list(sdk_archive.sdk_members(source,'emulator'))

    def test_runtime_archives_preserve_modes_and_reject_cross_component_paths(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'tools.zip'
            entry=zipfile.ZipInfo('emulator/emulator');entry.external_attr=(0o100755<<16)
            with zipfile.ZipFile(source,'w') as output:output.writestr(entry,b'program')
            sdk_archive.unpack_sdk(source,root/'sdk','emulator')
            self.assertEqual((root/'sdk/emulator/emulator').read_bytes(),b'program')
            if os.name=='posix':self.assertEqual((root/'sdk/emulator/emulator').stat().st_mode&0o777,0o755)
            for name,mode in [('../outside',0o100644),('platform-tools/adb',0o100755),('emulator/link',0o120777),('emulator/pipe',0o010644)]:
                with zipfile.ZipFile(source,'w') as output:
                    entry=zipfile.ZipInfo(name);entry.external_attr=mode<<16;output.writestr(entry,b'bad')
                with self.assertRaises(ValueError):sdk_archive.unpack_sdk(source,root/'invalid','emulator')
                self.assertFalse((root/'outside').exists())

    def test_duplicate_and_oversized_members_fail_before_activation(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'tools.zip'
            with zipfile.ZipFile(source,'w') as output:output.writestr('emulator/bin',b'one')
            with patch('android_sdk_archive.SDK_MAX_BYTES',2),self.assertRaisesRegex(ValueError,'budget'):
                sdk_archive.unpack_sdk(source,root/'sdk','emulator')
            self.assertFalse((root/'sdk').exists())
            with zipfile.ZipFile(source,'w') as output:
                output.writestr('emulator/bin',b'one')
                with self.assertWarns(UserWarning):output.writestr('emulator/bin',b'two')
            with self.assertRaisesRegex(ValueError,'Invalid'):sdk_archive.unpack_sdk(source,root/'sdk','emulator')
            self.assertFalse((root/'sdk').exists())

    def test_missing_components_cannot_produce_a_complete_factory_receipt(self):
        full=['web','native-services','phone-apps','container-host','js-services','guest-apps','android-runtime','container-images','installer-tools']
        self.assertTrue(factory.checked_delivery(full)['complete_device_bundle'])
        for name in full:
            with self.assertRaisesRegex(ValueError,'Incomplete'):factory.checked_delivery([r for r in full if r!=name])

    def test_image_builds_use_unique_factory_references_and_preserve_existing_images(self):
        namespace=factory.image_namespace('a'*64,'b'*12)
        self.assertTrue(namespace.startswith('ai-phone-factory/'))
        for source,nonce in [('a'*63,'b'*12),('g'*64,'b'*12),('a'*64,'b'*11)]:
            with self.assertRaisesRegex(ValueError,'identity'):factory.image_namespace(source,nonce)
        with patch('linux_client_factory.docker_image',return_value={'Id':'sha256:'+'c'*64}),patch('linux_client_factory.run') as run:
            with self.assertRaisesRegex(ValueError,'exists'):factory.build_image(Path('/source'),'workspace',namespace+'/workspace:build')
            run.assert_not_called()


if __name__=='__main__':unittest.main()
