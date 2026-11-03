"""Public fixtures cannot conceal unknown keys, credentials, or changed bytes."""
import hashlib
import io
import unittest
from unittest.mock import patch
from client_resources import inspect_stream


class PublicVectorTests(unittest.TestCase):
    def fixture(self):
        header=b'-'*5+b'BEGIN '+b'PRIVATE KEY'+b'-'*5
        footer=b'-'*5+b'END '+b'PRIVATE KEY'+b'-'*5
        return header+b'\npublic-self-test-fixture\n'+footer+b'\n\0'

    def pins(self,body):
        return patch('public_vectors.known_vectors',return_value={len(body):{hashlib.sha256(body).hexdigest()}})

    def test_exact_public_string_passes_in_original_bytes_and_across_chunks(self):
        vector=self.fixture()
        for offset in [0,1024**2-15,1024**2-50,1024**2-4096-5]:
            body=b'x'*offset+vector+b' tail'
            with self.pins(vector):receipt=inspect_stream(io.BytesIO(body),'arbitrary-vendor-name')
            self.assertEqual(receipt,{'size':len(body),'sha256':hashlib.sha256(body).hexdigest()})

    def test_changed_public_string_or_terminator_is_still_rejected(self):
        vector=self.fixture()
        for body in [vector.replace(b'fixture',b'changed'),vector[:-1],vector[:-1]+b'x\0']:
            with self.pins(vector),self.assertRaisesRegex(ValueError,'Private material'):
                inspect_stream(io.BytesIO(body),'libgnutls.so.30')

    def test_short_stream_reads_do_not_reject_an_incomplete_public_fixture(self):
        class ShortReads(io.BytesIO):
            def read(self,size=-1):
                return super().read(min(size,13))
        vector=self.fixture()
        with self.pins(vector):receipt=inspect_stream(ShortReads(vector),'vendor')
        self.assertEqual(receipt['sha256'],hashlib.sha256(vector).hexdigest())

    def test_public_vector_never_exempts_adjacent_raw_or_utf16_credentials(self):
        vector=self.fixture();secret=b' sk-'+b'Q'*32
        for credential in [secret,secret.decode().encode('utf-16-le')]:
            for offset in [0,1024**2-len(vector)-7]:
                body=b'x'*offset+vector+credential
                with self.pins(vector),self.assertRaisesRegex(ValueError,'Private material'):
                    inspect_stream(io.BytesIO(body),'libgnutls.so.30',hashlib.sha256(body).hexdigest())
