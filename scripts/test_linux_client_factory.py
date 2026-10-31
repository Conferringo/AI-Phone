"""Full Linux factory contracts reject incomplete plans and unsafe SDK archives."""
import os
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest.mock import patch

import linux_client_factory as factory


class LinuxClientFactoryTests(unittest.TestCase):
    def test_runtime_archives_preserve_modes_and_reject_cross_component_paths(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'tools.zip'
            entry=zipfile.ZipInfo('emulator/emulator');entry.external_attr=(0o100755<<16)
            with zipfile.ZipFile(source,'w') as output:output.writestr(entry,b'program')
            factory.unpack_sdk(source,root/'sdk','emulator')
            self.assertEqual((root/'sdk/emulator/emulator').read_bytes(),b'program')
            if os.name=='posix':self.assertEqual((root/'sdk/emulator/emulator').stat().st_mode&0o777,0o755)
            for name,mode in [('../outside',0o100644),('platform-tools/adb',0o100755),('emulator/link',0o120777),('emulator/pipe',0o010644)]:
                with zipfile.ZipFile(source,'w') as output:
                    entry=zipfile.ZipInfo(name);entry.external_attr=mode<<16;output.writestr(entry,b'bad')
                with self.assertRaises(ValueError):factory.unpack_sdk(source,root/'invalid','emulator')
                self.assertFalse((root/'outside').exists())

    def test_duplicate_and_oversized_members_fail_before_activation(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'tools.zip'
            with zipfile.ZipFile(source,'w') as output:output.writestr('emulator/bin',b'one')
            with patch('linux_client_factory.SDK_MAX_BYTES',2),self.assertRaisesRegex(ValueError,'budget'):
                factory.unpack_sdk(source,root/'sdk','emulator')
            self.assertFalse((root/'sdk').exists())
            with zipfile.ZipFile(source,'w') as output:
                output.writestr('emulator/bin',b'one')
                with self.assertWarns(UserWarning):output.writestr('emulator/bin',b'two')
            with self.assertRaisesRegex(ValueError,'Invalid'):factory.unpack_sdk(source,root/'sdk','emulator')
            self.assertFalse((root/'sdk').exists())

    def test_missing_components_cannot_produce_a_complete_factory_receipt(self):
        full=['web','native-services','phone-apps','container-host','js-services','guest-apps','android-runtime','container-images','installer-tools']
        self.assertTrue(factory.checked_delivery(full)['complete_device_bundle'])
        for name in full:
            with self.assertRaisesRegex(ValueError,'Incomplete'):factory.checked_delivery([r for r in full if r!=name])

    def test_image_builds_use_unique_factory_references_and_preserve_existing_images(self):
        namespace=factory.image_namespace('a'*64,'b'*12)
        self.assertTrue(namespace.startswith('ai-phone-factory/'))
        with patch('linux_client_factory.docker_image',return_value={'Id':'sha256:'+'c'*64}),patch('linux_client_factory.run') as run:
            with self.assertRaisesRegex(ValueError,'exists'):factory.build_image(Path('/source'),'workspace',namespace+'/workspace:build')
            run.assert_not_called()


if __name__=='__main__':unittest.main()
