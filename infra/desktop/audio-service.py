#!/usr/bin/env python3
"""Own and restart the guest audio daemon without restarting desktop applications."""
import fcntl
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def stop_child(child):
    if child.poll() is not None:
        return
    child.terminate()
    try:
        child.wait(timeout=2)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()


def supervise(config):
    stopped = False
    def stop(_signum, _frame):
        nonlocal stopped
        stopped = True
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    env = {key: value for key, value in os.environ.items() if key != 'PULSE_SERVER'}
    command = ['pulseaudio', '--daemonize=no', '--exit-idle-time=-1', '--disallow-exit=yes',
               '--log-target=stderr', '-n', '--file=' + config]
    while not stopped:
        child = subprocess.Popen(command, env=env)
        try:
            while not stopped and child.poll() is None:
                time.sleep(.2)
        finally:
            stop_child(child)
        if not stopped:
            time.sleep(1)


def main():
    runtime = Path(os.environ['XDG_RUNTIME_DIR'])
    with (runtime / 'audio-supervisor.lock').open('w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        config = sys.argv[1] if len(sys.argv) > 1 else '/etc/ai-phone-pulse.pa'
        supervise(config)


if __name__ == '__main__':
    main()
