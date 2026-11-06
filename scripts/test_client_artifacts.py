import json
import gzip
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from client_artifacts import inspect_payload, record
from release_package import SECRET


class ClientPayloadTests(unittest.TestCase):
    def test_record_keeps_installer_eligibility_separate_from_missing_device_resources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'config').mkdir();(root/'apps/client/src-tauri').mkdir(parents=True)
            (root/'config/release-policy.json').write_text('{"version":"fixture"}')
            (root/'config/host-bootstrap.json').write_text('{"hosts":{}}')
            for name in ['bun.lock','apps/client/src-tauri/Cargo.lock']:(root/name).write_bytes(b'fixture lock')
            artifact=root/'AI-Phone.deb';artifact.write_bytes(b'inspected client fixture')
            with patch('client_artifacts.notice_manifest',return_value={}),patch('prepare_client_host.payload_components',return_value=['web']):
                value=record(artifact,'linux','amd64',[],root=root)
            self.assertTrue(value['installer_delivery']['github_asset_eligible'])
            self.assertTrue(value['installer_delivery']['single_file'])
            self.assertFalse(value['complete_device_bundle'])
            self.assertFalse(value['remote_upload'])
            self.assertIn('android-runtime',value['missing_runtime_components'])
            saved=json.loads(artifact.with_suffix('.deb.manifest.json').read_text())
            self.assertEqual(saved,value)

    def fixture(self, root):
        folder=root/'apps/client/src-tauri';folder.mkdir(parents=True)
        (root/'LICENSE').write_text('Test licence')
        (folder/'tauri.conf.json').write_text(json.dumps({'bundle':{'resources':{'../../../LICENSE':'legal/LICENSE'}}}))
        return {'assets/legal/LICENSE':b'Test licence'}

    def test_legal_files_must_match_and_private_tokens_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);files=self.fixture(root)
            inspect_payload(files,root)
            for bad in [{}, {'assets/legal/LICENSE':b'stale'},
                        {**files,'private':b'"sk-'+b'X'*32+b'"'},
                        {**files,'private':('"sk-'+'X'*32+'"').encode('utf-16-le')}]:
                with self.assertRaises(ValueError):inspect_payload(bad,root)

    def test_css_keyword_substrings_are_not_api_keys(self):
        self.assertIsNone(SECRET.search(b'mask-typevector-effectscriptingexternalResourcesRequired'))
        self.assertIsNotNone(SECRET.search(b'api_key="sk-'+b'X'*32+b'"'))

    def test_native_source_archive_cannot_be_transparently_expanded_or_renamed(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);files=self.fixture(root);native=root/'apps/client/src-tauri';payload=native/'source.payload';payload.write_bytes(gzip.compress(b'reviewed source archive'))
            config=native/'tauri.conf.json';value=json.loads(config.read_text());value['bundle']['resources']['source.payload']='host/source.payload';config.write_text(json.dumps(value))
            inspect_payload({**files,'assets/host/source.payload':payload.read_bytes()},root)
            with self.assertRaisesRegex(ValueError,'Missing legal'):
                inspect_payload({**files,'assets/host/source.tar':gzip.decompress(payload.read_bytes())},root)
            with self.assertRaisesRegex(ValueError,'Stale legal'):
                inspect_payload({**files,'assets/host/source.payload':gzip.decompress(payload.read_bytes())},root)



if __name__=='__main__':unittest.main()
