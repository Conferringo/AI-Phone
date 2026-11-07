"""Real tar reader budgets; runtime boundaries are covered by native Rust tests."""
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from test_wsl_backend import rootfs
import wsl_backend


class BudgetedHeaders(io.BytesIO):
    def __init__(self):
        def header(name,size=0):
            row=tarfile.TarInfo(name);row.size=size;return row.tobuf()
        prefix=header(wsl_backend.BODY,1)+b'x'.ljust(512,b'\0')+header(wsl_backend.MANIFEST,1)+b'x'.ljust(512,b'\0')
        self.budget=2*1024+512
        super().__init__(prefix+header('unexpected-third')+header('unread')*10000+b'\0'*1024)
        self.maximum_read=0;self.bytes_read=0
    def read(self,size=-1):
        end=self.tell()+(size if size>=0 else len(self.getvalue())-self.tell())
        if end>self.budget:raise AssertionError('Reader scanned beyond the third outer header')
        result=super().read(size);self.maximum_read=max(self.maximum_read,self.tell());self.bytes_read+=len(result);return result


class WslArchiveBudgetTests(unittest.TestCase):
    def test_rejects_third_header_without_reading_remaining_outer_archive(self):
        source=BudgetedHeaders()
        with self.assertRaisesRegex(ValueError,'members'):wsl_backend.verify_stream(source)
        self.assertEqual(source.maximum_read,source.budget)
        self.assertEqual(source.bytes_read,3*512)

    def test_pax_header_is_rejected_before_reading_its_declared_body(self):
        header=tarfile.TarInfo('pax');header.type=tarfile.XHDTYPE;header.size=2**40
        raw=io.BytesIO(header.tobuf())
        with self.assertRaisesRegex(ValueError,'members'):wsl_backend.verify_stream(raw)
        self.assertEqual(raw.tell(),512)

    def test_valid_capsule_never_looks_up_members_by_string(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);body=root/'rootfs.tar.gz';rootfs(body)
            payload=root/'payload';wsl_backend.publish_capsule(body,payload,{})
            with patch.object(tarfile.TarFile,'getmembers',side_effect=AssertionError('Unbounded enumeration')),patch.object(tarfile.TarFile,'getmember',side_effect=AssertionError('String member lookup')):
                self.assertFalse(wsl_backend.verify_capsule(payload)['complete_device_bundle'])


if __name__=='__main__':unittest.main()
