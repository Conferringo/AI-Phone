"""Real, local Linux subprocess qualification; no network, VM, device or build."""
import contextlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import linux_bootstrap as bootstrap

REAL_POPEN = subprocess.Popen
REAL_KILLPG = os.killpg
GROUPS = {}
PROCESSES = {}
SIGNALS = []

LEADER = r'''
import json, os, signal, sys, time
from pathlib import Path
root=Path(sys.argv[1]); mode=sys.argv[2]
child=os.fork()
if child == 0:
    def graceful_stop(*unused):
        time.sleep(.05)
        (root/'cleanup.done').write_text('released')
        os._exit(0)
    signal.signal(signal.SIGTERM, graceful_stop if mode == 'graceful' else signal.SIG_IGN)
    status=Path('/proc/self/stat').read_text().rsplit(')', 1)[1].split()
    record={'pid':os.getpid(),'start_ticks':status[19],'group':os.getpgrp(),'stdout_inode':os.fstat(1).st_ino}
    pending=root/'child.next';pending.write_text(json.dumps(record));pending.replace(root/'child.json')
    while True:
        (root/'heartbeat').write_text(str(time.monotonic_ns()))
        time.sleep(.01)
while not (root/'heartbeat').exists(): time.sleep(.002)
if mode == 'graceful':
    os.kill(child,signal.SIGTERM)
    os._exit(0)
if mode == 'zero': os._exit(0)
if mode == 'nonzero': os._exit(7)
while True: time.sleep(.01)
'''
SENTINEL = 'import time; time.sleep(30)'


def deny_foreign(event, arguments):
    if event.startswith('socket.') or event in {'os.system', 'os.exec', 'os.posix_spawn'}:
        raise AssertionError('External operation forbidden: '+event)
    if event == 'subprocess.Popen':
        executable, arguments, unused_cwd, unused_env = arguments
        if executable != sys.executable or arguments[:4] != [sys.executable, '-I', '-S', '-B']:
            raise AssertionError('Only fixed owned Python child programs are allowed')
        if arguments[4:6] not in (['-c', LEADER], ['-c', SENTINEL]):
            raise AssertionError('Unrecognized child program')
    if event == 'os.killpg':
        pid, sig = arguments
        process = GROUPS.get(pid)
        if process is None or process.returncode is not None:
            raise AssertionError('Cannot signal an unowned or reaped group')
        SIGNALS.append((pid, sig))
    if event == 'os.kill':
        pid, unused_sig = arguments
        process = PROCESSES.get(pid)
        if process is None or process.returncode is not None:
            raise AssertionError('Cannot signal an unowned or reaped child')


sys.addaudithook(deny_foreign)


def status(pid):
    try:
        data=Path('/proc', str(pid), 'stat').read_text().rsplit(')', 1)[1].split()
        return data[0], data[19]
    except FileNotFoundError:
        return None


def wait_until(predicate, seconds=3):
    deadline=time.monotonic()+seconds
    while not predicate():
        if time.monotonic() >= deadline:
            raise AssertionError('Owned fixture did not reach its expected state')
        time.sleep(.005)


@unittest.skipUnless(sys.platform == 'linux', 'Real qualification requires Linux /proc and pidfds')
class OwnedSessionQualification(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory(prefix='emulator-session-qualify-')
        self.addCleanup(self.temporary.cleanup)
        self.root=Path(self.temporary.name)
        self.environment=bootstrap.isolated_environment(self.root)
        self.log=(self.root/'owned.log').open('wb')
        self.addCleanup(self.log.close)
        self.command=bootstrap.SourceCommand(self.root,self.environment,self.log)
        # A tiny owned tmpfs is intentional; disk policy is exercised explicitly below.
        capacity=patch.object(bootstrap.shutil,'disk_usage',return_value=SimpleNamespace(free=4*bootstrap.GIB))
        capacity.start()
        self.addCleanup(capacity.stop)
        self.created=[]
        self.addCleanup(self.cleanup_children)
        self.sentinel=self.start([sys.executable,'-I','-S','-B','-c',SENTINEL],start_new_session=True)
        self.signal_start=len(SIGNALS)

    def start(self, arguments, **kwargs):
        process=REAL_POPEN(arguments, **kwargs)
        self.created.append(process)
        PROCESSES[process.pid]=process
        if kwargs.get('start_new_session'):
            GROUPS[process.pid]=process
        return process

    def child_record(self):
        wait_until(lambda:(self.root/'child.json').exists())
        value=json.loads((self.root/'child.json').read_text())
        self.assertEqual(value['stdout_inode'],os.fstat(self.log.fileno()).st_ino)
        return value

    def child_running(self):
        record=self.child_record()
        value=status(record['pid'])
        return value is not None and value[1] == record['start_ticks'] and value[0] not in {'Z','X'}

    def cleanup_children(self):
        path=self.root/'child.json'
        if path.exists():
            record=json.loads(path.read_text())
            current=status(record['pid'])
            if current and current[1] == record['start_ticks'] and current[0] not in {'Z','X'}:
                try:
                    fd=os.pidfd_open(record['pid'])
                except ProcessLookupError:
                    pass
                else:
                    try:
                        if status(record['pid']) == current:
                            signal.pidfd_send_signal(fd,signal.SIGKILL)
                    finally:
                        os.close(fd)
        owned_ids=[process.pid for process in self.created]
        for process in reversed(self.created):
            if process.returncode is None:
                try:
                    process.kill()
                except ProcessLookupError:
                    pass
                process.wait(timeout=3)
            GROUPS.pop(process.pid,None)
            PROCESSES.pop(process.pid,None)
        record=json.loads(path.read_text()) if path.exists() else {}
        current=status(record['pid']) if record else None
        print(json.dumps({'case':self._testMethodName,'owned_pids':owned_ids,
                          'child_pid':record.get('pid'),'child_start_ticks':record.get('start_ticks'),
                          'child_group':record.get('group'),'child_stopped':current is None or current[0] in {'Z','X'},
                          'all_direct_children_reaped':all(process.returncode is not None for process in self.created),
                          'group_signals':[(pid,int(sig)) for pid,sig in SIGNALS[self.signal_start:]]}),flush=True)

    def execute(self,mode,wait=None,start=None,graceful=False):
        arguments=[sys.executable,'-I','-S','-B','-c',LEADER,str(self.root),mode]
        with contextlib.ExitStack() as context:
            context.enter_context(patch.object(bootstrap.subprocess,'Popen',side_effect=start or self.start))
            if wait:
                context.enter_context(patch.object(self.command,'wait',side_effect=wait))
            return self.command.run(arguments,self.root,timeout=3,**({'graceful':True} if graceful else {}))

    def assert_finished_and_sentinel_alive(self):
        wait_until(lambda:not self.child_running())
        heartbeat=(self.root/'heartbeat').read_bytes()
        time.sleep(.03)
        self.assertEqual((self.root/'heartbeat').read_bytes(),heartbeat)
        self.assertIsNone(self.sentinel.poll())
        self.assertTrue(any(pid == self.created[-1].pid and sig == signal.SIGKILL
                            for pid,sig in SIGNALS[self.signal_start:]))

    def test_zero_exit_reaps_group_without_normal_grace(self):
        self.execute('zero')
        self.assert_finished_and_sentinel_alive()
        self.assertEqual(self.created[-1].returncode,0)
        self.assertNotIn((self.created[-1].pid,signal.SIGTERM),SIGNALS[self.signal_start:])

    def test_nonzero_exit_keeps_primary_and_reaps_group(self):
        with self.assertRaisesRegex(bootstrap.BuildFailure,'command_failed'):
            self.execute('nonzero')
        self.assert_finished_and_sentinel_alive()
        self.assertEqual(self.created[-1].returncode,7)

    def test_display_cleanup_finishes_before_the_completed_group_returns(self):
        self.execute('graceful',graceful=True)
        self.assertFalse(self.child_running())
        self.assertEqual((self.root/'cleanup.done').read_text(),'released')
        self.assert_finished_and_sentinel_alive()

    def test_cancel_reaps_group_and_preserves_cancel(self):
        def cancel(process,timeout):
            self.child_record()
            raise KeyboardInterrupt
        with self.assertRaises(KeyboardInterrupt):
            self.execute('linger',wait=cancel)
        self.assert_finished_and_sentinel_alive()

    def test_disk_floor_reaps_group_without_retry(self):
        actual_wait=self.command.wait
        def low_disk(process,timeout):
            self.child_record()
            with patch.object(bootstrap.shutil,'disk_usage',return_value=SimpleNamespace(free=0)):
                return actual_wait(process,timeout)
        with self.assertRaisesRegex(bootstrap.BuildInterrupted,'disk_floor'):
            self.execute('linger',wait=low_disk)
        self.assert_finished_and_sentinel_alive()
        self.assertEqual(len(self.created),2)

    def test_cleanup_failure_keeps_both_errors_and_unreaped_owner(self):
        def cancel(process,timeout):
            self.child_record()
            raise KeyboardInterrupt
        def deny_kill(pid,sig):
            if sig == signal.SIGKILL:
                raise PermissionError('owned fixture denied final signal')
            return REAL_KILLPG(pid,sig)
        with patch.object(bootstrap.os,'killpg',side_effect=deny_kill):
            with self.assertRaises(BaseExceptionGroup) as raised:
                self.execute('linger',wait=cancel)
        self.assertIsInstance(raised.exception.exceptions[0],KeyboardInterrupt)
        self.assertIsInstance(raised.exception.exceptions[1],PermissionError)
        process=self.created[-1]
        self.assertIsNone(process.returncode)
        self.assertEqual(os.waitid(os.P_PID,process.pid,os.WEXITED|os.WNOHANG|os.WNOWAIT).si_pid,process.pid)
        self.command.stop(process)
        self.assert_finished_and_sentinel_alive()

    def test_foreign_group_is_not_signalled(self):
        def cancel(process,timeout):
            self.child_record()
            raise KeyboardInterrupt
        def foreign_group(arguments,**kwargs):
            kwargs['start_new_session']=False
            return self.start(arguments,**kwargs)
        with self.assertRaises(BaseExceptionGroup) as raised:
            self.execute('linger',wait=cancel,start=foreign_group)
        self.assertIsInstance(raised.exception.exceptions[0],KeyboardInterrupt)
        self.assertRegex(str(raised.exception.exceptions[1]),'process_owner')
        self.assertFalse(SIGNALS[self.signal_start:])
        self.assertIsNone(self.sentinel.poll())


if __name__ == '__main__':
    unittest.main(verbosity=2)
