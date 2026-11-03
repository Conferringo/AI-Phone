import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import package_delivery as delivery


class DeliveryTests(unittest.TestCase):
    def fixture(self,root):
        (root/'dist').mkdir();(root/'dist/installer.deb').write_bytes(b'complete installer')
        return {'file':'installer.deb','size':18,'sha256':'a'*64,'delivery':{'complete_device_bundle':True}}

    def test_small_complete_installer_keeps_the_single_file_handoff(self):
        with tempfile.TemporaryDirectory() as temporary,patch.object(delivery,'native') as native:
            artifact=self.fixture(Path(temporary))
            self.assertEqual(delivery.prepare(Path(temporary),artifact),{'format':'single','file':'installer.deb','size':18,'sha256':'a'*64})
            native.assert_not_called()

    def test_native_volume_handoff_verifies_the_inspected_installer_identity(self):
        with tempfile.TemporaryDirectory() as temporary,patch.object(delivery,'ASSET_LIMIT',18):
            root=Path(temporary);artifact=self.fixture(root);folder=root/'dist/installer.deb.volumes';folder.mkdir()
            parts=[{'name':'installer.deb.part-0001','size':18,'sha256':'a'*64}]
            (folder/'installer.deb.volumes.json').write_text(json.dumps({'parts':parts}))
            sealed={'manifest':'installer.deb.volumes.json','manifest_sha256':'b'*64};verified={'verified':True,'sha256':'a'*64,'size':18}
            with patch.object(delivery,'native',side_effect=[sealed,verified]) as native:
                value=delivery.prepare(root,artifact)
                self.assertEqual(value['format'],'volumes');self.assertEqual(value['parts'],parts)
                self.assertEqual(value['manifest'],'installer.deb.volumes/installer.deb.volumes.json')
                self.assertEqual(native.call_args_list[0].args,(root,'split',root/'dist/installer.deb',folder,'a'*64))
                self.assertEqual(native.call_args_list[1].args,(root,'verify',folder/'installer.deb.volumes.json','b'*64))
            for bad in ({**verified,'verified':False},{**verified,'sha256':'c'*64},{**verified,'size':19}):
                with patch.object(delivery,'native',side_effect=[sealed,bad]),self.assertRaises(ValueError):delivery.prepare(root,artifact)

    def test_incomplete_changed_or_escaping_artifact_is_not_passed_to_native_tool(self):
        with tempfile.TemporaryDirectory() as temporary,patch.object(delivery,'native') as native:
            root=Path(temporary);artifact=self.fixture(root)
            for bad in ({**artifact,'file':'../installer.deb'},{**artifact,'delivery':{'complete_device_bundle':False}},{**artifact,'size':19}):
                with self.assertRaises(ValueError):delivery.prepare(root,bad)
            native.assert_not_called()


if __name__=='__main__':unittest.main()
