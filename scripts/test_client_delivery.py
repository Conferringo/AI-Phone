"""Delivery facts describe embedded resources, independently of release approval."""
import unittest
from client_delivery import delivery_metadata

FULL=['web','native-services','phone-apps','container-host','js-services','guest-apps','android-runtime','container-images','installer-tools']

class ClientDeliveryTests(unittest.TestCase):
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

    def test_component_presence_does_not_claim_an_unimplemented_host(self):
        for host in ('linux-x64','linux-arm64','windows-x64','android-arm64'):
            value=delivery_metadata(FULL,host);self.assertTrue(value['server_included']);self.assertFalse(value['complete_device_bundle']);self.assertIn('host-runtime',value['missing_runtime_components'])
        for invalid in (None,'web',['unknown'],['web','web']):
            with self.assertRaises(ValueError):delivery_metadata(invalid,'macos-arm64')

if __name__=='__main__':unittest.main()
