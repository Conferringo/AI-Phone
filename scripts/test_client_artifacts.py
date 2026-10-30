import json
import gzip
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile
from client_artifacts import inspect_apk, inspect_payload
from release_package import SECRET


class ClientPayloadTests(unittest.TestCase):
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

    def test_apk_requires_real_arm64_elf(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);files=self.fixture(root);data=bytearray(64)
            data[:5]=b'\x7fELF\x02';struct.pack_into('<H',data,18,183)
            files['lib/arm64-v8a/libclient.so']=bytes(data)
            apk=root/'test.apk'
            for valid in [True,False]:
                if not valid:files['lib/arm64-v8a/libclient.so']=b'wrong architecture'
                with zipfile.ZipFile(apk,'w') as z:
                    for name,content in files.items():z.writestr(name,content)
                if valid:self.assertEqual(len(inspect_apk(apk,root)),2)
                else:
                    with self.assertRaises(ValueError):inspect_apk(apk,root)


if __name__=='__main__':unittest.main()
