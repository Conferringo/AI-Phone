"""Native guest test wiring, with every Docker call replaced by a closed fixture."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('native_media_contract', Path(__file__).with_name('test_desktop_media.py'))
media = importlib.util.module_from_spec(spec)
spec.loader.exec_module(media)


class NativeWiringTests(unittest.TestCase):
    def run_setup(self, header, env):
        calls = []

        def run(argv, **kwargs):
            calls.append(argv)
            if argv[:2] == ['docker', 'run']:
                self.assertIn('--network', argv)
                self.assertEqual(argv[argv.index('--network') + 1], 'none')
                self.assertIn('--init', argv)
                self.assertIn('--rm', argv)
                self.assertIn('ai-phone.test=media', argv)
            elif argv[:2] == ['docker', 'cp']:
                self.assertTrue(Path(argv[2]).name in {'pulse.pa', 'media-capture.py', 'audio-service.py'} or Path(argv[2]).is_file())
            elif argv[:2] == ['docker', 'exec']:
                self.assertTrue(argv[-4:-1] == ['head', '-c', '4'] or argv[-3:-1] == ['chmod', '755'])
            elif argv[:3] != ['docker', 'rm', '-f']:
                self.fail('unmocked external operation')
            return subprocess.CompletedProcess(argv, 0, stdout=header, stderr=b'')

        def spawn(argv, **kwargs):
            calls.append(argv)
            self.assertEqual(argv[:2], ['docker', 'exec'])
            self.assertNotIn('-t', argv)
            self.assertNotIn('python3', argv)
            self.assertTrue(argv[-2:] == ['audio-supervise', '/tmp/pulse.pa'] or argv[-2:] == ['capture', 'audio'])
            return object()

        with patch.dict(os.environ, env, clear=True), patch.object(media.subprocess, 'run', side_effect=run), \
                patch.object(media.subprocess, 'Popen', side_effect=spawn) as spawned, \
                patch.object(media.DesktopMediaTest, 'wait_audio') as ready:
            try:
                media.DesktopMediaTest.setUpClass()
                instance = media.DesktopMediaTest()
                instance.capture()
            except AssertionError:
                spawned.assert_not_called()
                ready.assert_not_called()
                raise
            finally:
                media.DesktopMediaTest.doClassCleanups()
        return calls, spawned.call_count, ready.call_count

    def test_image_native_bridge_is_used_for_supervision_and_capture_without_python_copy(self):
        calls, spawned, ready = self.run_setup(b'\x7fELF', {'AI_PHONE_TEST_DESKTOP_IMAGE': 'owned-matched-rust-image'})
        self.assertEqual(spawned, 2)
        self.assertEqual(ready, 1)
        copied = [argv[2] for argv in calls if argv[:2] == ['docker', 'cp']]
        self.assertEqual(len(copied), 1)
        self.assertTrue(copied[0].endswith('/infra/desktop/pulse.pa'))
        self.assertTrue(all('media-capture.py' not in str(argv) and 'audio-service.py' not in str(argv) for argv in calls))
        captures = [argv for argv in calls if argv[-2:] == ['capture', 'audio']]
        self.assertEqual(captures[0][-3], '/usr/local/bin/desktop-bridge')
        self.assertIn('-i', captures[0])

    def test_old_python_image_is_rejected_before_any_supervisor_starts(self):
        with self.assertRaisesRegex(AssertionError, 'matched Rust guest image'):
            self.run_setup(b'#!/u', {})

    def test_explicit_linux_override_is_nonce_owned_and_host_binary_is_unchanged(self):
        with tempfile.TemporaryDirectory() as owned:
            binary = Path(owned) / 'linux-bridge'
            binary.write_bytes(b'\x7fELFfixture-only')
            before = binary.stat()
            calls, spawned, ready = self.run_setup(b'\x7fELF', {'AI_PHONE_TEST_GUEST_BINARY': str(binary)})
            after = binary.stat()
            self.assertEqual((before.st_ino, before.st_uid, before.st_gid, before.st_mode), (after.st_ino, after.st_uid, after.st_gid, after.st_mode))
            self.assertEqual(binary.read_bytes(), b'\x7fELFfixture-only')
            self.assertEqual((spawned, ready), (2, 1))
            copied = [argv for argv in calls if argv[:2] == ['docker', 'cp']]
            self.assertEqual(len(copied), 2)
            self.assertTrue(copied[1][-1].split(':', 1)[1].startswith('/tmp/desktop-bridge-'))

    def test_explicit_macos_binary_is_rejected_before_execution(self):
        with tempfile.TemporaryDirectory() as owned:
            binary = Path(owned) / 'mac-bridge'
            binary.write_bytes(b'\xcf\xfa\xed\xfe')
            with self.assertRaisesRegex(AssertionError, 'matching native Linux ELF'):
                self.run_setup(b'\x7fELF', {'AI_PHONE_TEST_GUEST_BINARY': str(binary)})


class LeaseQualificationTests(unittest.TestCase):
    def run_lease_case(self, elapsed, returncode):
        import io
        from types import SimpleNamespace
        child = SimpleNamespace(stdin=io.BytesIO(), stdout=io.BytesIO(b'\0' * 9600),
                                stderr=io.BytesIO(), returncode=returncode,
                                wait=lambda **options: returncode)
        case = media.DesktopMediaTest('test_lost_heartbeat_expires')
        with patch.object(case, 'capture', return_value=child), \
                patch.object(case, 'assert_no_capture'), \
                patch.object(media.time, 'monotonic', side_effect=[0.0, elapsed]):
            case.test_lost_heartbeat_expires()

    def test_early_exit_cannot_qualify_as_a_five_second_lease(self):
        with self.assertRaises(AssertionError):
            self.run_lease_case(1.0, 0)

    def test_failed_capture_cannot_qualify_as_lease_expiry(self):
        with self.assertRaises(AssertionError):
            self.run_lease_case(5.2, 1)

    def test_clean_capture_survives_the_full_lease(self):
        self.run_lease_case(5.2, 0)


if __name__ == '__main__':
    unittest.main()
