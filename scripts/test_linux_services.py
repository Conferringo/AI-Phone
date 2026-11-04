"""Real supervisor processes in isolated temporary data; no guest/user fixtures."""
import json
import hashlib
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from concurrent.futures import ThreadPoolExecutor
from unittest.mock import patch

import linux_services

from linux_services import start_unit, stop_unit, unit_state


@unittest.skipUnless(sys.platform.startswith('linux'), 'Linux /proc ownership contract')
class LinuxServiceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root/'data').mkdir()
        self.fixture = self.root/'worker.py'
        self.fixture.write_text('import os,time\nfrom pathlib import Path\nPath("data/child.next").write_text(str(os.getpid()))\nos.replace("data/child.next","data/child")\nwhile True:time.sleep(.05)\n')
        self.argv = [sys.executable, str(self.fixture)]
        binary = Path(__file__).resolve().parent.parent/'target/debug/host-resources'
        self.supervisor = patch('linux_services.supervisor_command',return_value=[str(binary)])
        self.supervisor.start();self.addCleanup(self.supervisor.stop)

    def tearDown(self):
        stop_unit(self.root, 'fixture')
        self.temp.cleanup()

    def wait_child(self, previous=None):
        deadline=time.monotonic()+8
        while time.monotonic()<deadline:
            if (self.root/'data/child').is_file():
                pid=int((self.root/'data/child').read_text())
                if pid!=previous and Path('/proc/'+str(pid)).exists():return pid
            time.sleep(.05)
        self.fail('Owned child did not become ready')

    def test_resume_keeps_pid_and_restart_recovers_only_owned_child(self):
        (self.root/'data/chat-fixture').write_text('keep')
        self.assertTrue(start_unit(self.root,'fixture',self.argv,{}))
        child=self.wait_child();before=unit_state(self.root,'fixture')['pid']
        self.assertFalse(start_unit(self.root,'fixture',self.argv,{}))
        self.assertEqual(before,unit_state(self.root,'fixture')['pid'])
        os.kill(child,signal.SIGKILL)
        replacement=self.wait_child(child)
        self.assertNotEqual(child,replacement)
        self.assertTrue(stop_unit(self.root,'fixture'))
        self.assertFalse(unit_state(self.root,'fixture'))
        self.assertEqual((self.root/'data/chat-fixture').read_text(),'keep')

    def test_source_supervisor_is_ready_before_workers_and_independent_of_assistant_builds(self):
        from linux_host import prepare_supervision
        self.supervisor.stop()
        built=Path(__file__).resolve().parent.parent/'target/debug/host-resources'
        compiler=[]
        def build(args):
            compiler.append(args)
            target=self.root/'target/debug/host-resources';target.parent.mkdir(parents=True)
            shutil.copy2(built,target)
        prepare_supervision(self.root,False,build)
        self.assertEqual(compiler,[['cargo','build','-p','ai-phone-host-runtime','--bin','host-resources','--locked']])
        self.assertTrue(start_unit(self.root,'fixture',self.argv,{}))
        child=self.wait_child();receipt=unit_state(self.root,'fixture')
        self.assertEqual(receipt['program'],str(self.root/'target/debug/host-resources'))
        (self.root/'target/debug/ai-phone-assistant').write_bytes(b'later model server build')
        prepare_supervision(self.root,False,build)
        self.assertEqual(len(compiler),1)
        self.assertEqual(unit_state(self.root,'fixture')['pid'],receipt['pid'])
        self.assertTrue(Path('/proc',str(child)).exists())
        self.assertTrue(stop_unit(self.root,'fixture'))

    def test_a_live_unit_is_not_replaced_by_a_different_command(self):
        start_unit(self.root,'fixture',self.argv,{})
        child=self.wait_child()
        with self.assertRaisesRegex(ValueError,'different service configuration'):
            start_unit(self.root,'fixture',[sys.executable,'-c','raise SystemExit(7)'],{})
        self.assertTrue(Path('/proc/'+str(child)).exists())

    def test_stale_pid_receipt_cannot_kill_an_unrelated_process(self):
        unrelated=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])
        try:
            folder=self.root/'data/services';folder.mkdir()
            (folder/'fixture.json').write_text(json.dumps({'pid':unrelated.pid,'start_ticks':'wrong','root':str(self.root)}))
            self.assertFalse(stop_unit(self.root,'fixture'))
            self.assertIsNone(unrelated.poll())
        finally:unrelated.terminate();unrelated.wait()

    def test_native_child_receives_environment_and_never_inherits_the_supervisor_lock(self):
        self.fixture.write_text('import os,time\nfrom pathlib import Path\nPath("data/environment").write_text(os.environ["NATIVE_FIXTURE"])\nPath("data/child.next").write_text(str(os.getpid()))\nos.replace("data/child.next","data/child")\nwhile True:time.sleep(.05)\n')
        self.assertTrue(start_unit(self.root,'fixture',self.argv,{'NATIVE_FIXTURE':'中文'}))
        child=self.wait_child();receipt=unit_state(self.root,'fixture')
        self.assertEqual(receipt['supervisor'],'rust-v1')
        self.assertEqual((self.root/'data/environment').read_text(),'中文')
        descriptors=[os.readlink(path) for path in (Path('/proc')/str(child)/'fd').iterdir()]
        self.assertNotIn(str(self.root/'data/services/fixture.lock'),descriptors)
        self.assertEqual(os.readlink(Path('/proc')/str(child)/'fd/0'),'/dev/null')
        self.assertTrue(stop_unit(self.root,'fixture'))
        import fcntl
        with (self.root/'data/services/fixture.lock').open('r+') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)

    def test_stop_waits_for_the_owned_process_after_receipt_removal(self):
        import fcntl
        folder=self.root/'data/services';folder.mkdir()
        receipt=folder/'fixture.json';release=self.root/'release';unpublished=self.root/'unpublished'
        code=('import os,signal,time\nfrom pathlib import Path\n'
              'def stop(*args):\n Path("data/services/fixture.json").unlink()\n Path("unpublished").touch()\n'
              ' while not Path("release").exists():time.sleep(.01)\n raise SystemExit(0)\n'
              'signal.signal(signal.SIGTERM,stop)\nPath("ready").touch()\n'
              'while True:time.sleep(.01)\n')
        with (folder/'fixture.lock').open('w+') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            child=subprocess.Popen([sys.executable,'-c',code,str(linux_services.__file__),'fixture'],
                                   cwd=self.root,stdin=lock,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        try:
            deadline=time.monotonic()+3
            while not (self.root/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
            self.assertTrue((self.root/'ready').is_file())
            receipt.write_text(json.dumps({'pid':child.pid,'start_ticks':linux_services.process_start(child.pid),'root':str(self.root)}))
            with ThreadPoolExecutor(max_workers=1) as executor:
                result=executor.submit(stop_unit,self.root,'fixture')
                try:
                    while not unpublished.exists() and time.monotonic()<deadline:time.sleep(.01)
                    self.assertTrue(unpublished.exists())
                    # The receipt is gone but the real owned process still holds its lock.
                    with self.assertRaises(TimeoutError):result.result(timeout=.2)
                    with (folder/'fixture.lock').open('r+') as lock:
                        with self.assertRaises(BlockingIOError):fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
                finally:release.touch()
                self.assertTrue(result.result(timeout=3))
            child.wait(timeout=3)
            with (folder/'fixture.lock').open('r+') as lock:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        finally:
            release.touch()
            if child.poll() is None:child.kill()
            child.wait(timeout=3)

    def test_stop_during_restart_backoff_does_not_start_a_replacement(self):
        self.fixture.write_text('from pathlib import Path\np=Path("data/exits")\np.write_text(p.read_text()+"exit\\n" if p.exists() else "exit\\n")\nraise SystemExit(7)\n')
        start_unit(self.root,'fixture',self.argv,{})
        until=time.monotonic()+5
        while not (self.root/'data/exits').exists() and time.monotonic()<until:time.sleep(.02)
        self.assertTrue((self.root/'data/exits').exists())
        self.assertTrue(stop_unit(self.root,'fixture'))
        self.assertEqual((self.root/'data/exits').read_text(),'exit\n')
        self.assertFalse(unit_state(self.root,'fixture'))

    def test_term_resistant_owned_child_is_reaped_with_the_existing_five_second_grace(self):
        self.fixture.write_text('import os,signal,time\nfrom pathlib import Path\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\nPath("data/child.next").write_text(str(os.getpid()))\nos.replace("data/child.next","data/child")\nwhile True:time.sleep(.05)\n')
        start_unit(self.root,'fixture',self.argv,{})
        child=self.wait_child()
        self.assertTrue(stop_unit(self.root,'fixture'))
        self.assertFalse(Path('/proc/'+str(child)).exists())
        self.assertFalse(unit_state(self.root,'fixture'))

    def test_changed_spec_and_foreign_stdin_cannot_start_a_worker(self):
        import fcntl
        folder=self.root/'data/services';folder.mkdir()
        spec=json.dumps({'command':self.argv,'environment':{}}).encode()
        (folder/'fixture.spec.json').write_bytes(spec)
        binary=Path(__file__).resolve().parent.parent/'target/debug/host-resources'
        with (folder/'fixture.lock').open('w+') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            command=[binary,'--supervise',self.root,'fixture','a'*64,'0'*64]
            result=subprocess.run(command,stdin=lock,capture_output=True,text=True,timeout=5)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('service_spec_changed',result.stderr)
            command[-1]=hashlib.sha256(spec).hexdigest()
            result=subprocess.run(command,stdin=subprocess.DEVNULL,capture_output=True,text=True,timeout=5)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('service_supervision_lock',result.stderr)
            with (folder/'foreign.lock').open('w+') as foreign:
                result=subprocess.run(command,stdin=foreign,capture_output=True,text=True,timeout=5)
                self.assertNotEqual(result.returncode,0)
                self.assertIn('service_supervision_lock',result.stderr)
        self.assertFalse((self.root/'data/child').exists())
        self.assertFalse((folder/'fixture.json').exists())


class ReadOnlyServiceStateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='service-state-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.folder = self.root/'data/services'
        self.value = {'format':1,'pid':42,'start_ticks':'123','root':str(self.root),'spec_sha256':'fixture'}
        self.identity = patch('linux_services.process_start',return_value='123')
        self.identity.start();self.addCleanup(self.identity.stop)
        self.command = patch('linux_services.Path.read_bytes',return_value=str(Path(linux_services.__file__).resolve()).encode()+b'\0fixture\0')
        self.command.start();self.addCleanup(self.command.stop)

    def write_receipt(self, suffix=''):
        self.folder.mkdir(parents=True,exist_ok=True)
        path = self.folder/'fixture.json'
        path.write_text(json.dumps(self.value)+suffix)
        return path

    def test_absent_inspection_and_stop_never_create_private_state(self):
        self.assertIsNone(unit_state(self.root,'fixture'))
        self.assertFalse((self.root/'data').exists())
        self.assertFalse(stop_unit(self.root,'fixture'))
        self.assertFalse((self.root/'data').exists())

    def test_authentic_receipt_preserves_identity_and_rejects_pid_reuse(self):
        self.write_receipt()
        self.assertEqual(unit_state(self.root,'fixture'),self.value)
        with patch('linux_services.process_start',return_value='reused'):
            self.assertIsNone(unit_state(self.root,'fixture'))
        with patch('linux_services.Path.read_bytes',return_value=b'other-service\0'):
            self.assertIsNone(unit_state(self.root,'fixture'))

    def test_rust_receipt_requires_matching_executable_and_exact_owned_arguments(self):
        program='/fixture/service-host'
        self.value.update(supervisor='rust-v1',program=program,spec_content_sha256='content')
        self.write_receipt()
        args=[program,'--supervise',str(self.root),'fixture','fixture','content']
        raw=b'\0'.join(part.encode() for part in args)+b'\0'
        with patch('linux_services.Path.read_bytes',return_value=raw),patch('linux_services.os.readlink',return_value=program):
            self.assertEqual(unit_state(self.root,'fixture'),self.value)
            with patch('linux_services.os.readlink',return_value='/unrelated/process'):
                self.assertIsNone(unit_state(self.root,'fixture'))
            for index in [1,2,3,4,5]:
                changed=list(args);changed[index]='unrelated'
                with patch('linux_services.Path.read_bytes',return_value=b'\0'.join(part.encode() for part in changed)+b'\0'):
                    self.assertIsNone(unit_state(self.root,'fixture'))
        with patch('linux_services.Path.read_bytes',return_value=b'other-service\0'):
            self.assertIsNone(unit_state(self.root,'fixture'))

    def test_unverified_process_cannot_authenticate_an_omitted_start_time(self):
        self.value.pop('start_ticks')
        self.write_receipt()
        with patch('linux_services.process_start',return_value=None):
            self.assertIsNone(unit_state(self.root,'fixture'))

    def test_oversize_symlink_fifo_and_non_object_receipts_are_rejected(self):
        path = self.write_receipt(' '*9000)
        self.assertIsNone(unit_state(self.root,'fixture'))
        path.unlink()
        target = self.root/'foreign.json'
        target.write_text(json.dumps(self.value))
        path.symlink_to(target)
        self.assertIsNone(unit_state(self.root,'fixture'))
        path.unlink();os.mkfifo(path)
        self.assertIsNone(unit_state(self.root,'fixture'))
        path.unlink();path.write_text('[]')
        self.assertIsNone(unit_state(self.root,'fixture'))

    def test_opened_receipt_is_read_once_when_path_is_replaced(self):
        path = self.write_receipt()
        foreign = self.root/'foreign.json'
        foreign.write_text(json.dumps(self.value)+' '*9000)
        real_open = os.open
        opens = []
        def replace_after_open(name, flags, *args, **kwargs):
            descriptor = real_open(name,flags,*args,**kwargs)
            if str(name).endswith('fixture.json'):
                opens.append((flags,kwargs.get('dir_fd')))
                path.unlink();path.symlink_to(foreign)
            return descriptor
        with patch('linux_services.os.open',side_effect=replace_after_open):
            self.assertEqual(unit_state(self.root,'fixture'),self.value)
        self.assertEqual(len(opens),1)
        self.assertTrue(opens[0][0]&os.O_NOFOLLOW)
        self.assertIsNotNone(opens[0][1])

    def test_service_directory_replacement_cannot_redirect_opened_receipt(self):
        self.write_receipt()
        foreign = self.root/'foreign'
        foreign.mkdir();(foreign/'fixture.json').write_text(json.dumps(self.value)+' '*9000)
        moved = self.root/'old-services'
        real_open = os.open
        replaced = []
        def replace_directory(name, flags, *args, **kwargs):
            descriptor = real_open(name,flags,*args,**kwargs)
            if Path(name)==self.folder:
                self.folder.rename(moved);self.folder.symlink_to(foreign)
                replaced.append(True)
            return descriptor
        with patch('linux_services.os.open',side_effect=replace_directory):
            self.assertEqual(unit_state(self.root,'fixture'),self.value)
        self.assertEqual(replaced,[True])


class NativeSupervisorLaunchTests(unittest.TestCase):
    def test_controller_transfers_the_existing_lock_and_binds_exact_spec_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve()
            environment={'LANG':'中文','FIXTURE':'kept'}
            with patch('linux_services.supervisor_command',return_value=['/fixture/native']) as program, \
                 patch('linux_services.subprocess.Popen') as launch, \
                 patch('linux_services.unit_state',return_value={'pid':7319}):
                self.assertTrue(start_unit(root,'fixture',['/fixture/worker','with spaces'],environment))
                program.assert_called_once_with(root)
                spec_path=root/'data/services/fixture.spec.json'
                spec=json.loads(spec_path.read_bytes())
                canonical=hashlib.sha256(json.dumps(spec,sort_keys=True).encode()).hexdigest()
                raw=hashlib.sha256(spec_path.read_bytes()).hexdigest()
                self.assertEqual(launch.call_args.args[0],['/fixture/native','--supervise',str(root),'fixture',canonical,raw])
                self.assertIsInstance(launch.call_args.kwargs['stdin'],int)
                self.assertNotIn('pass_fds',launch.call_args.kwargs)


if __name__=='__main__':unittest.main()
