"""The browser byte relay is bounded and preserves binary CDP frames."""
import os
import threading
import unittest
import subprocess
import sys
from pathlib import Path
from unittest.mock import patch
from browser_pipe import write_all,relay,high_pipe,run_relay,stop_guest,reserve_debug_fds


class BrowserPipeTests(unittest.TestCase):
    @unittest.skipIf(os.name=='nt','Linux guest signal/process-group contract')
    def test_a_cancelled_relay_exits_even_while_its_input_is_blocked(self):
        code='''import os,signal,threading
from browser_pipe import run_relay
reader,writer=os.pipe()
threading.Timer(0.03,lambda:os.kill(os.getpid(),signal.SIGTERM)).start()
run_relay(type('Child',(),{'poll':lambda self:None})(),[(reader,os.open(os.devnull,os.O_WRONLY))])
print('closed')
'''
        env={**os.environ,'PYTHONPATH':str(Path(__file__).absolute().parent)}
        result=subprocess.run([sys.executable,'-c',code],capture_output=True,text=True,timeout=2,env=env)
        self.assertEqual(result.returncode,0);self.assertEqual(result.stdout.strip(),'closed')

    @unittest.skipIf(os.name=='nt','Linux guest signal/process-group contract')
    def test_guest_can_finish_a_delayed_flush_after_termination(self):
        from pathlib import Path
        import tempfile
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)/'flushed'
            code='''import signal,time,sys
from pathlib import Path
def finish(*_):
 time.sleep(0.05);Path(sys.argv[1]).write_text('flushed');raise SystemExit(0)
signal.signal(signal.SIGTERM,finish)
print('ready',flush=True)
while True:time.sleep(1)
'''
            child=subprocess.Popen([sys.executable,'-c',code,str(output)],stdout=subprocess.PIPE,start_new_session=True)
            try:
                self.assertEqual(child.stdout.readline(),b'ready\n');first=True
                def wait(timeout=None):
                    nonlocal first
                    if first:first=False;raise subprocess.TimeoutExpired('injected initial flush deadline',3)
                    return child.wait(timeout=timeout)
                stop_guest(type('Guest',(),{'pid':child.pid,'wait':staticmethod(wait)})())
                self.assertEqual(output.read_text(),'flushed');self.assertEqual(child.returncode,0)
            finally:
                if child.poll() is None:child.kill();child.wait()
                child.stdout.close()

    def test_debug_descriptors_are_reserved_without_closing_already_owned_fds(self):
        with patch('browser_pipe.os.open',side_effect=[3,4]),patch('browser_pipe.os.dup2') as duplicate,patch('browser_pipe.os.close') as close:
            reserve_debug_fds();duplicate.assert_not_called();close.assert_not_called()
        with patch('browser_pipe.os.open',side_effect=[8,9]),patch('browser_pipe.os.dup2') as duplicate,patch('browser_pipe.os.close') as close:
            reserve_debug_fds();self.assertEqual([call.args for call in duplicate.call_args_list],[(8,3),(9,4)]);self.assertEqual([call.args for call in close.call_args_list],[(8,),(9,)])

    def test_duplex_child_exchanges_bytes_without_a_tcp_debug_port(self):
        source_r,source_w=os.pipe();target_r,target_w=os.pipe();received=[]
        child=subprocess.Popen([sys.executable,'-c','import os; os.write(1,os.read(0,1024)); os.read(0,1)'],stdin=subprocess.PIPE,stdout=subprocess.PIPE)
        def exchange():
            write_all(source_w,b'fixture\0reply');received.append(os.read(target_r,1024));os.close(source_w)
        worker=threading.Thread(target=exchange,daemon=True);worker.start()
        try:
            with patch('browser_pipe.signal.signal'):run_relay(child,[(source_r,child.stdin.fileno()),(child.stdout.fileno(),target_w)])
            child.stdin.close();child.wait(timeout=3);worker.join(3)
            self.assertFalse(worker.is_alive());self.assertEqual(received,[b'fixture\0reply'])
        finally:
            if child.poll() is None:child.kill();child.wait()
            child.stdout.close()
            for fd in (source_r,target_r,target_w):os.close(fd)

    @unittest.skipIf(os.name=='nt','Linux guest process-group contract')
    def test_guest_shutdown_waits_for_flush_then_escalates_only_its_process_group(self):
        from unittest.mock import Mock
        child=Mock(pid=123)
        with patch('browser_pipe.os.killpg') as kill:stop_guest(child);kill.assert_not_called()
        child=Mock(pid=123);child.wait.side_effect=[subprocess.TimeoutExpired('fixture',3),subprocess.TimeoutExpired('fixture',1),None]
        with patch('browser_pipe.os.killpg') as kill:
            stop_guest(child);self.assertEqual([call.args[0] for call in kill.call_args_list],[123,123]);self.assertEqual(child.wait.call_count,3)

    @unittest.skipIf(os.name=='nt','guest pipe duplication is Linux-only')
    def test_failed_second_duplicate_closes_the_first_owned_descriptor(self):
        import fcntl
        duplicate=fcntl.fcntl;owned=[]
        def fail_second(*args):
            if owned:raise OSError('second duplicate failed')
            fd=duplicate(*args);owned.append(fd);return fd
        with patch('fcntl.fcntl',side_effect=fail_second),self.assertRaises(OSError):high_pipe()
        with self.assertRaises(OSError):os.fstat(owned[0])

    def test_partial_writes_keep_every_byte_and_zero_write_fails(self):
        written=[]
        def write(fd,value):written.append(bytes(value[:3]));return min(3,len(value))
        data='中文\0payload'.encode()
        with patch('browser_pipe.os.write',side_effect=write):write_all(4,data)
        self.assertEqual(b''.join(written),data)
        with patch('browser_pipe.os.write',return_value=1):write_all(4,b'x')
        with patch('browser_pipe.os.write',return_value=0),self.assertRaises(BrokenPipeError):write_all(4,b'x')

    def test_binary_relay_streams_beyond_pipe_capacity_and_marks_eof(self):
        source_r,source_w=os.pipe();target_r,target_w=os.pipe();stop=threading.Event();data=(b'fixture\0'+bytes(range(256)))*1024;received=bytearray()
        thread=threading.Thread(target=relay,args=(source_r,target_w,stop));thread.start()
        def consume():
            while len(received)<len(data):received.extend(os.read(target_r,65536))
        reader=threading.Thread(target=consume);reader.start()
        try:
            write_all(source_w,data);os.close(source_w);source_w=-1
            thread.join(3);reader.join(3)
            self.assertFalse(thread.is_alive());self.assertFalse(reader.is_alive());self.assertEqual(bytes(received),data);self.assertTrue(stop.is_set())
        finally:
            for fd in [source_r,source_w,target_r,target_w]:
                if fd>=0:os.close(fd)


if __name__=='__main__':unittest.main()
