"""Real guest recovery/cleanup contract; only an isolated labelled container."""
import os
from pathlib import Path
import subprocess
import time
import threading
import unittest
import uuid

ROOT = Path(__file__).resolve().parent.parent


@unittest.skipUnless(os.environ.get('AI_PHONE_TEST_DESKTOP_MEDIA') == '1', 'requires isolated Docker fixture')
class DesktopMediaTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.container = 'ai-phone-media-test-' + uuid.uuid4().hex[:10]
        subprocess.run(['docker', 'run', '-d', '--rm', '--init', '--network', 'none',
                        '--label', 'ai-phone.test=media', '--name', cls.container,
                        '--entrypoint', 'sh', 'ai-phone-desktop:20261028', '-c',
                        'mkdir -p /tmp/ai-runtime; chown ai:ai /tmp/ai-runtime; chmod 700 /tmp/ai-runtime; exec sleep 300'],
                       check=True, stdout=subprocess.DEVNULL)
        cls.addClassCleanup(lambda: subprocess.run(['docker', 'rm', '-f', cls.container], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        for name in ['media-capture.py', 'audio-service.py', 'pulse.pa']:
            subprocess.run(['docker', 'cp', str(ROOT / 'infra/desktop' / name),
                            f'{cls.container}:/tmp/{name}'], check=True)
        cls.supervisor = subprocess.Popen(['docker', 'exec', '-u', 'ai', cls.container,
                                          'python3', '/tmp/audio-service.py', '/tmp/pulse.pa'],
                                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        cls.wait_audio()

    @classmethod
    def tearDownClass(cls):
        subprocess.run(['docker', 'rm', '-f', cls.container], check=True, stdout=subprocess.DEVNULL)
        cls.supervisor.wait(timeout=5)

    @classmethod
    def run_guest(cls, *args):
        return subprocess.run(['docker', 'exec', '-u', 'ai', cls.container, *args],
                              capture_output=True, text=True)

    @classmethod
    def wait_audio(cls):
        for _ in range(50):
            if cls.run_guest('pactl', 'list', 'short', 'sources').stdout.find('aiphone.monitor') >= 0:
                return
            time.sleep(.1)
        raise AssertionError('audio service did not recover')

    def test_daemon_crash_recovers_with_default_monitor(self):
        before = self.run_guest('pgrep', '-x', 'pulseaudio').stdout.strip()
        self.assertTrue(before)
        self.run_guest('kill', '-KILL', before)
        self.wait_audio()
        after = self.run_guest('pgrep', '-x', 'pulseaudio').stdout.strip()
        self.assertNotEqual(before, after)
        self.assertEqual(self.run_guest('pactl', 'get-default-sink').stdout.strip(), 'aiphone')

    def assert_no_capture(self):
        for _ in range(40):
            if self.run_guest('pgrep', '-x', 'ffmpeg').returncode == 1:
                return
            time.sleep(.1)
        self.fail('guest capture leaked after its owner closed')

    def capture(self):
        return subprocess.Popen(['docker', 'exec', '-i', '-u', 'ai', self.container,
                                 'python3', '/tmp/media-capture.py', 'audio'],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def test_disconnect_and_host_crash_release_guest(self):
        for crash in [False, True, False]:
            child = self.capture()
            self.assertGreater(len(child.stdout.read(9600)), 0)
            drain = threading.Thread(target=child.stdout.read)
            drain.start()
            if crash:
                child.kill()
            else:
                child.stdin.close()
            child.wait(timeout=8)
            drain.join(timeout=2)
            child.stdin.close()
            child.stdout.close()
            child.stderr.close()
            self.assert_no_capture()

    def test_lost_heartbeat_expires(self):
        child = self.capture()
        self.assertGreater(len(child.stdout.read(9600)), 0)
        drain = threading.Thread(target=child.stdout.read)
        drain.start()
        child.wait(timeout=8)
        drain.join(timeout=2)
        child.stdin.close()
        child.stdout.close()
        child.stderr.close()
        self.assert_no_capture()


if __name__ == '__main__':
    unittest.main()
