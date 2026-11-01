"""Real supervisor processes in isolated temporary data; no guest/user fixtures."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
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


if __name__=='__main__':unittest.main()
