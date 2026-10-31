"""One Android readiness owner; diagnostics never change its guest settings."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time


class BootFailure(RuntimeError):
    def __init__(self, phase, message):
        self.phase = phase
        super().__init__(message)


def execute(command, timeout):
    try:
        return subprocess.run(command, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None


def observation(result):
    if result is None:
        return {'return_code': None, 'timed_out_or_unavailable': True, 'boot_completed': 'unknown'}
    value = result.stdout.strip()
    return {'return_code': result.returncode, 'timed_out_or_unavailable': False,
            'boot_completed': value if result.returncode == 0 and value in ('', '0', '1') else 'unknown'}


def wait_boot(run, clock, sleep, state, child):
    deadline = clock() + 120
    seen = None
    while clock() < deadline:
        result = run('shell', 'getprop', 'sys.boot_completed')
        state['observations'] += 1
        state['last_boot'] = observation(result)
        if result and result.returncode == 0 and result.stdout.strip() == '1':
            return
        current = child()
        if current.get('verified'):
            identity = current.get('identity')
            if seen is not None and identity != seen:
                raise BootFailure('owned_child_exited', 'Owned Android process exited before boot completed')
            if identity is not None:
                seen = identity
        sleep(1)
    raise BootFailure('boot_failed', 'Android did not finish booting within 120 seconds')


def configure_ready(run):
    run('shell', 'settings', 'put', 'system', 'show_touches', '1')
    touches = run('shell', 'settings', 'get', 'system', 'show_touches')
    if not touches or touches.returncode != 0 or touches.stdout.strip() != '1':
        raise BootFailure('settings_failed', 'Native Android touch indicators could not be enabled')
    overlays = run('shell', 'cmd', 'overlay', 'list')
    if overlays:
        for line in overlays.stdout.splitlines():
            cutout = line.startswith('[x] com.android.internal.display.cutout.emulation.')
            pixel = line in ('[x] com.android.internal.emulation.pixel_7', '[x] com.android.systemui.emulation.pixel_7')
            if cutout or pixel:
                run('shell', 'cmd', 'overlay', 'disable', '--user', '0', line.split()[-1])


def wait_root(run, observe_only, sleep):
    if not observe_only:
        run('root')
    for _ in range(20):
        result = run('shell', 'id', '-u')
        if result and result.returncode == 0 and result.stdout.strip() == '0':
            if observe_only:
                print('READY: existing Android/root observed; settings unchanged')
            else:
                configure_ready(run)
                print('READY: real Android boot completed; control channel uid=0; no emulated display cutout')
            return
        sleep(0.5)
    raise BootFailure('root_failed', 'Android is not root-ready')


def ready(adb, observe_only=False, *, invoke=execute, clock=time.monotonic,
          sleep=time.sleep, child=lambda: {'verified': False}, state=None):
    state = state if state is not None else {'observations': 0, 'last_boot': {}}
    def run(*args):
        try:
            return invoke([*adb, *args], 6)
        except OSError as error:
            raise BootFailure('adb_unavailable', 'Android control program unavailable') from error
    wait_boot(run, clock, sleep, state, child)
    wait_root(run, observe_only, sleep)


def main(argv=None):
    from android_boot_diagnostics import collect, owned_process, resources
    args = sys.argv[1:] if argv is None else argv
    if not 1 <= len(args) <= 2 or (len(args) == 2 and args[1] != '--observe-only'):
        raise SystemExit('Invalid Android readiness invocation')
    root = Path(os.environ['AI_PHONE_PROJECT_ROOT'])
    adb = [args[0], '-P', '15037', '-s', os.environ.get('AI_PHONE_SERIAL', 'emulator-5564')]
    before = resources()
    state = {'observations': 0, 'last_boot': {}}
    started = time.monotonic()
    try:
        ready(adb, len(args) == 2, child=lambda: owned_process(root), state=state)
    except BootFailure as failure:
        # Keep the original failure even when optional diagnosis is unavailable.
        try:
            report = collect(root, adb, failure.phase, state, before, started)
            encoded = json.dumps(report, sort_keys=True)
            if len(encoded.encode()) > 16384:
                raise ValueError('diagnostic budget')
            print('AI_PHONE_ANDROID_BOOT ' + encoded, flush=True)
        except Exception:
            print('AI_PHONE_ANDROID_BOOT {"format":1,"diagnostic":"unavailable"}', flush=True)
        raise SystemExit(str(failure)) from None
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
