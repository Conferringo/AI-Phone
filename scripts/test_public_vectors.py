"""Public fixtures cannot conceal unknown keys, credentials, or changed bytes."""
import hashlib
import io
import unittest
from unittest.mock import patch
from client_resources import inspect_stream


class PublicVectorTests(unittest.TestCase):
    def test_public_security_key_algorithm_name_does_not_exempt_other_api_keys(self):
        span=b'sk-'+b'ecdsa-sha2-nistp256-cert-v01@openssh.com\0'
        with self.pins(span):
            self.assertEqual(inspect_stream(io.BytesIO(span),'parser')['sha256'],hashlib.sha256(span).hexdigest())
        for body in [span.replace(b'v01',b'v02'),span+b' sk-'+b'Q'*32]:
            with self.pins(span),self.assertRaisesRegex(ValueError,'Private material'):
                inspect_stream(io.BytesIO(body),'parser')

    def test_public_parser_marker_requires_the_entire_known_literal_span(self):
        header=b'-'*5+b'BEGIN OPENSSH '+b'PRIVATE KEY'+b'-'*5
        span=header+b'\0'*5+b'Public parser diagnostic (no header)\0'
        with self.pins(span):
            value=inspect_stream(io.BytesIO(span),'public-parser')
        self.assertEqual(value['sha256'],hashlib.sha256(span).hexdigest())
        for changed in [header+b'\0',span.replace(b'diagnostic',b'changed'),
                        header+b'\0\nunknown-key-material\n'+span[len(header):]]:
            with self.pins(span),self.assertRaisesRegex(ValueError,'Private material'):
                inspect_stream(io.BytesIO(changed),'public-parser')
        for credential in [self.fixture(),self.fixture().decode().encode('utf-16-le')]:
            with self.pins(span),self.assertRaisesRegex(ValueError,'Private material'):
                inspect_stream(io.BytesIO(span+credential),'public-parser')

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
