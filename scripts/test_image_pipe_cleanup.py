"""Body/pipe/child combined faults, with no real subprocess or image access."""
import io
import lzma
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import container_images as images
import build_container_images as builder


class Cancelled(BaseException):
    pass


class FaultPipe(io.BytesIO):
    def __init__(self, order, *, read_error=None, close_errors=()):
        super().__init__(b'owned export fixture')
        self.order=order;self.read_error=read_error;self.close_errors=list(close_errors)

    def read(self, size=-1):
        if self.read_error is not None:
            raise self.read_error
        return super().read(size)

    def close(self):
        self.order.append('close')
        if self.close_errors:
            raise self.close_errors.pop(0)
        return super().close()


class FaultSource(io.BytesIO):
    def __init__(self, error):
        super().__init__(b'owned import fixture');self.error=error

    def read(self, size=-1):
        if self.error is not None:
            raise self.error
        return super().read(size)


class ImagePipeCleanupTests(unittest.TestCase):
    def invoke(self, direction, primary=None, close_errors=(), stop_error=None):
        order=[]
        pipe=FaultPipe(order,read_error=primary if direction=='export' else None,close_errors=close_errors)
        self.addCleanup(io.BytesIO.close,pipe)
        child=SimpleNamespace(stdin=pipe,stdout=pipe,wait=Mock(return_value=0))

        def stop(value):
            self.assertIs(value,child);order.append('stop')
            if stop_error is not None:
                raise stop_error

        stopped=Mock(side_effect=stop)
        with patch.object(images.subprocess,'Popen',return_value=child) as started:
            with patch.object(images,'stop_child',stopped),patch.object(builder,'stop_child',stopped,create=True):
                if direction=='import':
                    result=images.docker_command(['image','load','--quiet'],FaultSource(primary))
                else:
                    with tempfile.NamedTemporaryFile() as raw:
                        result=builder.save_images(['owned-ref'],raw,'arm64')
                        raw.seek(0)
                        self.assertEqual(lzma.decompress(raw.read()),b'owned export fixture')
        self.assertEqual(started.call_count,1)
        return result,order,stopped

    def test_primary_and_both_cleanup_failures_are_preserved(self):
        for direction in ('import','export'):
            for error_type in (ValueError,OSError,Cancelled):
                with self.subTest(direction=direction,error=error_type.__name__):
                    primary=error_type('original body');closing=OSError('pipe close');stopping=RuntimeError('child stop')
                    with self.assertRaises(BaseExceptionGroup) as raised:
                        self.invoke(direction,primary,[closing],stopping)
                    expected=(primary,stopping,closing) if direction=='import' else (primary,closing,stopping)
                    self.assertEqual(raised.exception.exceptions,expected)

    def test_export_close_failure_still_stops_the_child(self):
        closing=OSError('stdout close');stopping=RuntimeError('child stop')
        with self.assertRaises(BaseExceptionGroup) as raised:
            self.invoke('export',close_errors=[closing],stop_error=stopping)
        self.assertEqual(raised.exception.exceptions,(closing,stopping))

    def test_import_stop_failure_still_closes_stdin(self):
        primary=Cancelled('interrupted copy');stopping=RuntimeError('child stop')
        order=[];pipe=FaultPipe(order)
        self.addCleanup(io.BytesIO.close,pipe)
        child=SimpleNamespace(stdin=pipe,wait=Mock(return_value=0))
        with patch.object(images.subprocess,'Popen',return_value=child),patch.object(images,'stop_child',side_effect=stopping) as stop:
            with self.assertRaises(BaseExceptionGroup) as raised:
                images.docker_command(['image','load'],FaultSource(primary))
        self.assertEqual(raised.exception.exceptions,(primary,stopping))
        self.assertTrue(pipe.closed)
        stop.assert_called_once_with(child)
        self.assertEqual(order,['close'])

    def test_body_only_failure_keeps_the_exact_exception(self):
        for direction in ('import','export'):
            for error_type in (ValueError,Cancelled):
                with self.subTest(direction=direction,error=error_type.__name__):
                    primary=error_type('body only')
                    with self.assertRaises(error_type) as raised:
                        self.invoke(direction,primary)
                    self.assertIs(raised.exception,primary)

    def test_success_preserves_stream_and_original_close_stop_order(self):
        for direction in ('import','export'):
            with self.subTest(direction=direction):
                result,order,stop=self.invoke(direction)
                self.assertEqual(result,'' if direction=='import' else None)
                self.assertEqual(order,['close','stop'])
                self.assertEqual(stop.call_count,1)

    def test_import_initial_close_and_both_cleanup_failures_are_kept(self):
        initial=OSError('initial stdin close');again=OSError('cleanup stdin close');stopping=RuntimeError('child stop')
        with self.assertRaises(BaseExceptionGroup) as raised:
            self.invoke('import',close_errors=[initial,again],stop_error=stopping)
        self.assertEqual(raised.exception.exceptions,(initial,stopping,again))

    def test_cleanup_only_stop_failure_is_not_hidden(self):
        for direction in ('import','export'):
            with self.subTest(direction=direction):
                stopping=RuntimeError('stop only')
                with self.assertRaises(BaseExceptionGroup) as raised:
                    self.invoke(direction,stop_error=stopping)
                self.assertEqual(raised.exception.exceptions,(stopping,))
