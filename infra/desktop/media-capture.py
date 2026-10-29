#!/usr/bin/env python3
"""A capture belongs to one connection. EOF or a lost lease reaps its process group."""
import os
import selectors
import signal
import subprocess
import sys
import time

LEASE_SECONDS = 5
BASE = ['ffmpeg', '-nostdin', '-hide_banner', '-loglevel', 'error']
COMMANDS = {
    'audio': BASE + ['-f', 'pulse', '-i', 'aiphone.monitor', '-ac', '2', '-ar', '48000', '-f', 's16le', 'pipe:1'],
    'screen': BASE + ['-f', 'x11grab', '-framerate', '12', '-video_size', '1280x800',
                      '-draw_mouse', '1', '-i', ':0', '-an', '-c:v', 'mjpeg', '-q:v', '5',
                      '-threads', '1', '-f', 'image2pipe', 'pipe:1'],
}


def reap(child):
    if child.poll() is not None:
        return
    os.killpg(child.pid, signal.SIGTERM)
    try:
        child.wait(timeout=1)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait()


def watch(child):
    deadline = time.monotonic() + LEASE_SECONDS
    with selectors.DefaultSelector() as selector:
        selector.register(sys.stdin, selectors.EVENT_READ)
        while child.poll() is None:
            if time.monotonic() >= deadline:
                return
            if selector.select(.2):
                if not os.read(sys.stdin.fileno(), 4096):
                    return
                deadline = time.monotonic() + LEASE_SECONDS


def main():
    command = COMMANDS[sys.argv[1]]
    child = subprocess.Popen(command, stdin=subprocess.DEVNULL, start_new_session=True)
    def stop(_signum, _frame):
        raise SystemExit(0)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        watch(child)
    finally:
        reap(child)


if __name__ == '__main__':
    main()
