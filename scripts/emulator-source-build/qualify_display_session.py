"""Real Linux Xvfb ownership checks; no network, source build, VM or user device."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import linux_bootstrap as bootstrap


def process_identity(pid):
    try:
        fields = Path('/proc', str(pid), 'stat').read_text().rsplit(')', 1)[1].split()
        return fields[0], fields[19]
    except FileNotFoundError:
        return None


def probe(record, mode):
    display = os.environ.get('DISPLAY', '')
    if not re.fullmatch(r':[0-9]{1,5}', display):
        raise RuntimeError('Headless display was not assigned')
    result = subprocess.run(['xset', '-display', display, '-q'],
                            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, check=False)
    if result.returncode:
        raise RuntimeError('The upstream display query failed')
    number = display[1:]
    pid = int(Path('/tmp', '.X' + number + '-lock').read_text().strip())
    identity = process_identity(pid)
    if identity is None or os.getsid(pid) != os.getsid(0):
        raise RuntimeError('Xvfb is outside the owned source-build session')
    pending=record.with_suffix('.next')
    with pending.open('x') as output:
        json.dump({'display': display, 'pid': pid, 'start_ticks': identity[1]}, output)
    pending.replace(record)
    if mode == 'hold':
        while True:
            time.sleep(.05)
    return 7 if mode == 'fail' else 0


@unittest.skipUnless(sys.platform == 'linux', 'Requires a real Linux X server and /proc')
class DisplaySessionQualification(unittest.TestCase):
    def test_success_failure_and_interruption_release_the_owned_display(self):
        for mode in ['success', 'fail', 'hold']:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(prefix='emulator-display-') as temporary:
                root = Path(temporary)
                environment = bootstrap.isolated_environment(root)
                record = root / 'display.json'
                arguments = bootstrap.with_display([
                    sys.executable, '-s', '-B', str(Path(__file__).resolve()),
                    '--probe', str(record), mode])
                with (root / 'display.log').open('wb') as log:
                    command = bootstrap.SourceCommand(root, environment, log)
                    if mode == 'hold':
                        def interrupt(process, timeout):
                            deadline = time.monotonic() + 5
                            while not record.exists():
                                if bootstrap.observed_exit(process) is not None or time.monotonic() >= deadline:
                                    raise AssertionError('The owned display probe did not start')
                                time.sleep(.01)
                            raise bootstrap.BuildInterrupted('fixture_interruption')
                        with patch.object(command, 'wait', side_effect=interrupt):
                            with self.assertRaisesRegex(bootstrap.BuildInterrupted, 'fixture_interruption'):
                                command.run(arguments, root,graceful=True)
                    elif mode == 'fail':
                        with self.assertRaisesRegex(bootstrap.BuildFailure, 'command_failed'):
                            command.run(arguments, root, timeout=10,graceful=True)
                    else:
                        command.run(arguments, root, timeout=10,graceful=True)
                self.assertTrue(record.is_file(), (root / 'display.log').read_text())
                value = json.loads(record.read_text())
                current = process_identity(value['pid'])
                self.assertTrue(current is None or current[1] != value['start_ticks']
                                or current[0] in {'Z', 'X'}, 'Owned Xvfb is still running')
                self.assertFalse(Path('/tmp/.X11-unix', 'X' + value['display'][1:]).exists(),
                                 'Owned X display socket remains')


if __name__ == '__main__':
    if sys.argv[1:2] == ['--probe']:
        sys.exit(probe(Path(sys.argv[2]), sys.argv[3]))
    unittest.main()
