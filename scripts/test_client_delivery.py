"""Delivery facts describe embedded resources, independently of release approval."""
import unittest
from pathlib import Path
import tempfile
from client_delivery import delivery_metadata,installer_metadata

FULL=['web','native-services','phone-apps','container-host','js-services','guest-apps','android-runtime','container-images','installer-tools']

class ClientDeliveryTests(unittest.TestCase):
    def test_complete_installer_is_one_regular_file_for_each_desktop_format(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for host,name,kind in [('macos-arm64','AI-Phone.dmg','dmg'),('windows-x64','AI-Phone.exe','exe'),
                                   ('linux-amd64','AI-Phone.deb','deb')]:
                with self.subTest(host=host,name=name):
                    path=root/name;path.write_bytes(b'inspected full runtime fixture')
                    value=installer_metadata(path,host)
                    self.assertTrue(value['single_file']);self.assertTrue(value['github_asset_eligible'])
                    self.assertFalse(value['user_assembly_required']);self.assertEqual(value['format'],kind)
                    self.assertEqual(value['file'],name);self.assertEqual(value['size'],path.stat().st_size)
                    self.assertNotIn('complete_device_bundle',value)
                    self.assertNotIn('publication_ready',value)

    def test_github_single_asset_boundary_is_not_replaced_by_a_volume_handoff(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'AI-Phone.dmg'
            for size,accepted in [(2_147_483_647,True),(2_147_483_648,False)]:
                with path.open('wb') as output:output.truncate(size)
                value=installer_metadata(path,'macos-arm64')
                self.assertTrue(value['single_file']);self.assertEqual(value['github_asset_eligible'],accepted)

    def test_parts_archives_and_cross_platform_installers_cannot_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for name in ['AI-Phone.dmg.part-0001','AI-Phone.zip','AI-Phone.exe','AI-Phone.dmg.volumes.json']:
                path=root/name;path.write_bytes(b'fixture')
                with self.assertRaises(ValueError):installer_metadata(path,'macos-arm64')
            path=root/'AI-Phone.dmg';path.touch()
            with self.assertRaises(ValueError):installer_metadata(path,'macos-arm64')
            path.unlink();path.mkdir()
            with self.assertRaises(ValueError):installer_metadata(path,'macos-arm64')
            path.rmdir();target=root/'target';target.write_bytes(b'fixture');path.symlink_to(target)
            with self.assertRaises(ValueError):installer_metadata(path,'macos-arm64')

    def test_source_or_web_payload_is_not_mislabeled_as_an_embedded_server(self):
        for components in ([],['web'],['native-services'],['web','native-services']):
            value=delivery_metadata(components,'macos-arm64')
            self.assertFalse(value['server_included']);self.assertFalse(value['complete_device_bundle'])

    def test_full_macos_payload_is_distinct_from_installation_or_publication_acceptance(self):
        value=delivery_metadata(FULL,'macos-arm64')
        self.assertTrue(value['server_included']);self.assertTrue(value['runtime_payload_complete']);self.assertTrue(value['complete_device_bundle'])
        self.assertEqual(value['missing_runtime_components'],[]);self.assertEqual(value['deployment_mode'],'embedded-local')
        self.assertNotIn('publication_ready',value);self.assertNotIn('runtime_acceptance',value)
        for missing in FULL:
            partial=delivery_metadata([name for name in FULL if name!=missing],'macos-arm64')
            self.assertFalse(partial['complete_device_bundle']);self.assertIn(missing,partial['missing_runtime_components'])

    def test_the_exact_service_set_is_sufficient_for_a_server_but_not_for_device_hosting(self):
        value=delivery_metadata(['web','native-services','js-services'],'macos-arm64')
        self.assertTrue(value['server_included']);self.assertFalse(value['complete_device_bundle'])

    def test_complete_linux_resources_describe_contents_without_claiming_runtime_acceptance(self):
        value=delivery_metadata(FULL,'linux-x64')
        self.assertTrue(value['runtime_payload_complete']);self.assertTrue(value['complete_device_bundle'])
        self.assertNotIn('runtime_acceptance',value)
        for missing in FULL:
            result=delivery_metadata([name for name in FULL if name!=missing],'linux-x64')
            self.assertFalse(result['complete_device_bundle']);self.assertIn(missing,result['missing_runtime_components'])

    def test_component_presence_does_not_claim_an_unimplemented_host(self):
        for host in ('linux-arm64',):
            value=delivery_metadata(FULL,host);self.assertTrue(value['server_included']);self.assertFalse(value['complete_device_bundle']);self.assertIn('host-runtime',value['missing_runtime_components'])
        windows=delivery_metadata(FULL,'windows-x64')
        self.assertFalse(windows['complete_device_bundle']);self.assertEqual(windows['missing_runtime_components'],['wsl-backend'])
        for invalid in (None,'web',['unknown'],['web','web']):
            with self.assertRaises(ValueError):delivery_metadata(invalid,'macos-arm64')

if __name__=='__main__':unittest.main()
