"""Small, offline fixtures for exact gzip/XZ envelope handling."""
import gzip
import hashlib
import io
import lzma
import struct
import unittest
import zlib

from build_capsule_codec import decoded, MEMORY_LIMIT


class CapsuleCodecTests(unittest.TestCase):
    def test_legacy_gzip_and_new_xz_are_seekable_and_close_without_owning_fd(self):
        body=bytes(range(251))*300
        for compressed in (gzip.compress(body),lzma.compress(body)):
            with self.subTest(magic=compressed[:6]):
                source=io.BytesIO(compressed)
                with decoded(source,len(body)) as raw:
                    self.assertEqual(raw.read(513),body[:513])
                    self.assertEqual(raw.seek(123),123)
                    self.assertEqual(raw.read(2048),body[123:2171])
                    self.assertEqual(raw.seek(0),0)
                    self.assertEqual(hashlib.file_digest(raw,'sha256').digest(),hashlib.sha256(body).digest())
                self.assertFalse(source.closed)

    def test_magic_metadata_and_raw_budgets_fail_closed(self):
        for data,expected in [(b'not compression',None),(gzip.compress(b'ok'),'xz'),(lzma.compress(b'ok'),'gzip')]:
            with self.subTest(data=data[:6]),self.assertRaises(ValueError):
                with decoded(io.BytesIO(data),10,expected=expected) as raw:raw.read(1)
        for encode in (gzip.compress,lzma.compress):
            with self.assertRaisesRegex(ValueError,'budget'):
                with decoded(io.BytesIO(encode(b'x'*4096)),4095) as raw:
                    while raw.read(1024):pass

    def test_footer_truncation_corruption_and_trailing_stream_are_not_ignored(self):
        for encode in (gzip.compress,lzma.compress):
            valid=encode(b'trailer integrity fixture')
            cases=[valid[:-1],valid[:-5]+bytes([valid[-5]^1])+valid[-4:],valid+b'junk',valid+valid]
            for data in cases:
                with self.subTest(magic=valid[:6],case=data[-12:]),self.assertRaises(ValueError):
                    with decoded(io.BytesIO(data),1024) as raw:raw.read(1)

    def test_xz_dictionary_over_128m_is_rejected_before_decoding_body(self):
        data=bytearray(lzma.compress(b'tiny fixture',preset=0))
        size=(data[12]+1)*4
        block=data[12:12+size]
        offset=block.index(b'\x21\x01')+2
        block[offset]=40
        block[-4:]=struct.pack('<I',zlib.crc32(block[:-4]))
        data[12:12+size]=block
        with self.assertRaisesRegex(ValueError,'compression'):
            with decoded(io.BytesIO(data),1024) as raw:raw.read(1)

    def test_cancellation_is_preserved_without_draining_or_closing_input(self):
        class Cancelled(BaseException):pass
        source=io.BytesIO(lzma.compress(b'x'*4096))
        with self.assertRaises(Cancelled):
            with decoded(source,4096) as raw:
                self.assertEqual(raw.read(1),b'x')
                raise Cancelled('fixture')
        self.assertFalse(source.closed)


if __name__=='__main__':unittest.main()
