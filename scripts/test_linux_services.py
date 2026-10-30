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


if __name__=='__main__':unittest.main()
