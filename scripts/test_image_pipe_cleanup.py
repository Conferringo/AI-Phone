"""Remaining Python factory export lifecycle; runtime import tests now run in Rust."""
import io
import gzip
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import container_images as images
import build_container_images as builder


class Cancelled(BaseException):
    pass


class FaultPipe(io.BytesIO):
    def __init__(self, order, read_error=None, close_errors=()):
        super().__init__(b'owned export fixture')
        self.order=order;self.read_error=read_error;self.close_errors=list(close_errors)

    def read(self, size=-1):
        if self.read_error is not None:raise self.read_error
        return super().read(size)

    def close(self):
        self.order.append('close')
        if self.close_errors:raise self.close_errors.pop(0)
        return super().close()


class ImagePipeCleanupTests(unittest.TestCase):
    def invoke(self, primary=None, close_errors=(), stop_error=None):
        order=[];pipe=FaultPipe(order,primary,close_errors);self.addCleanup(io.BytesIO.close,pipe)
        child=SimpleNamespace(stdout=pipe,wait=Mock(return_value=0))
        def stop(value):
            self.assertIs(value,child);order.append('stop')
            if stop_error is not None:raise stop_error
        stopped=Mock(side_effect=stop)
        with patch.object(builder.subprocess,'Popen',return_value=child) as started,patch.object(images,'stop_child',stopped):
            with tempfile.NamedTemporaryFile() as raw:
                result=builder.save_images(['owned-ref'],raw,'arm64');raw.seek(0)
                self.assertEqual(gzip.decompress(raw.read()),b'owned export fixture')
        self.assertEqual(started.call_count,1)
        return result,order,stopped

    def test_primary_and_both_cleanup_failures_are_preserved(self):
        for error_type in (ValueError,OSError,Cancelled):
            with self.subTest(error=error_type.__name__):
                primary=error_type('original body');closing=OSError('pipe close');stopping=RuntimeError('child stop')
                with self.assertRaises(BaseExceptionGroup) as raised:self.invoke(primary,[closing],stopping)
                self.assertEqual(raised.exception.exceptions,(primary,closing,stopping))

    def test_export_close_failure_still_stops_the_child(self):
        closing=OSError('stdout close');stopping=RuntimeError('child stop')
        with self.assertRaises(BaseExceptionGroup) as raised:self.invoke(close_errors=[closing],stop_error=stopping)
        self.assertEqual(raised.exception.exceptions,(closing,stopping))

    def test_body_only_failure_keeps_the_exact_exception(self):
        for error_type in (ValueError,Cancelled):
            primary=error_type('body only')
            with self.assertRaises(error_type) as raised:self.invoke(primary)
            self.assertIs(raised.exception,primary)

    def test_success_preserves_stream_and_close_stop_order(self):
        result,order,stop=self.invoke();self.assertIsNone(result);self.assertEqual(order,['close','stop']);self.assertEqual(stop.call_count,1)

    def test_cleanup_only_stop_failure_is_not_hidden(self):
        stopping=RuntimeError('stop only')
        with self.assertRaises(BaseExceptionGroup) as raised:self.invoke(stop_error=stopping)
        self.assertEqual(raised.exception.exceptions,(stopping,))
