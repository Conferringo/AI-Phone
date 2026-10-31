"""Own one serial-bound input setup intent under a nonblocking process lock."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from private_files import atomic_json
from installer_lock import acquire

PACKAGE = 'dev.aiphone.input'
IME = PACKAGE + '/.UnicodeIme'
ATTENTION = PACKAGE + '/.PhoneAttention'
NOTIFICATION = PACKAGE + '/.PhoneNotifications'
USER = '0'
REGISTRATION_SECONDS = 5.0
OBSERVE_INTERVAL = 0.1
COMPONENT = r'[A-Za-z0-9_.$]+/[A-Za-z0-9_.$]+'


def command(prefix, args, stage, timeout=10):
    try:
        result = subprocess.run([*prefix, *args], capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired, UnicodeError):
        raise RuntimeError('Android input ' + stage + ' failed: result unknown') from None
    if result.returncode:
        raise RuntimeError('Android input ' + stage + ' failed: result unknown')
    return result.stdout.strip()


def pending_record(path, serial):
    if re.fullmatch(r'[A-Za-z0-9_.:-]{1,128}', serial) is None:
        raise RuntimeError('Android input serial is invalid')
    if path.is_symlink() or path.parent.is_symlink():
        raise RuntimeError('Android input pending owner is invalid')
    if not path.exists():return None
    try:
        if not path.is_file() or path.stat().st_size > 4096:raise ValueError()
        value = json.loads(path.read_text())
        expected = {'format':1, 'package':PACKAGE, 'serial':serial, 'user':USER}
        if not isinstance(value, dict) or set(value) != set(expected) | {'stage'}:raise ValueError()
        if any(type(value[key]) is not type(item) or value[key] != item for key,item in expected.items()):raise ValueError()
        if value['stage'] not in ('install_unknown','configure'):raise ValueError()
        return value
    except (OSError, ValueError, UnicodeError):
        raise RuntimeError('Android input pending owner is invalid; preserved for review') from None


def save_pending(path, serial, stage):
    try:atomic_json(path, {'format':1,'package':PACKAGE,'serial':serial,'user':USER,'stage':stage})
    except (OSError, ValueError):
        raise RuntimeError('Android input pending intent could not be saved') from None


def package_present(adb):
    # A successful empty package list proves absence; transport failures never do.
    value = command(adb, ['shell','pm','list','packages','--user',USER,PACKAGE], 'package observation')
    rows = value.splitlines()
    if any(re.fullmatch(r'package:[A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)+', row) is None for row in rows):
        raise RuntimeError('Android input package observation is unknown')
    return 'package:' + PACKAGE in rows


def await_registration(adb, clock, pause):
    deadline = clock() + REGISTRATION_SECONDS
    while True:
        remaining = deadline - clock()
        if remaining <= 0:raise RuntimeError('Android input registration did not become ready')
        value = command(adb, ['shell','ime','list','-a','-s','--user',USER], 'registration', timeout=remaining)
        ids = value.splitlines()
        if any(re.fullmatch(COMPONENT, item) is None for item in ids):
            raise RuntimeError('Android input registration returned an invalid component list')
        if IME in ids:return
        pause(min(OBSERVE_INTERVAL, max(0, deadline - clock())))


def settings(adb, operation, key, *values):
    return command(adb, ['shell','settings','--user',USER,operation,'secure',key,*values], 'settings')


def components(value, *, ime=False):
    if value in ('','null'):return []
    rows = value.split(':')
    names = [row.split(';',1)[0] if ime else row for row in rows]
    if any(re.fullmatch(COMPONENT, name) is None for name in names):
        raise RuntimeError('Android input settings observation is invalid')
    return names


def has_component(names, wanted):
    def expanded(name):
        package, service = name.split('/',1)
        return package+'/'+(package+service if service.startswith('.') else service)
    return any(expanded(name) == expanded(wanted) for name in names)


def configure_ime(adb):
    if not has_component(components(settings(adb,'get','enabled_input_methods'),ime=True),IME):
        command(adb, ['shell','ime','enable','--user',USER,IME], 'enable')
        if not has_component(components(settings(adb,'get','enabled_input_methods'),ime=True),IME):
            raise RuntimeError('Android input enable was not confirmed')
    selected = components(settings(adb,'get','default_input_method'))
    if len(selected)>1:raise RuntimeError('Android input selection observation is invalid')
    if not has_component(selected,IME):
        command(adb, ['shell','ime','set','--user',USER,IME], 'selection')
        if not has_component(components(settings(adb,'get','default_input_method')),IME):
            raise RuntimeError('Android input selection was not confirmed')


def enable_setting(adb, key):
    current = settings(adb,'get',key)
    if current == '1':return
    if current not in ('0','null'):raise RuntimeError('Android input setting observation is invalid')
    settings(adb,'put',key,'1')
    if settings(adb,'get',key) != '1':raise RuntimeError('Android input setting was not confirmed')


def configure_services(adb):
    enable_setting(adb,'show_ime_with_hard_keyboard')
    if not has_component(components(settings(adb,'get','enabled_notification_listeners')),NOTIFICATION):
        command(adb, ['shell','cmd','notification','allow_listener',NOTIFICATION,USER], 'notifications')
        if not has_component(components(settings(adb,'get','enabled_notification_listeners')),NOTIFICATION):
            raise RuntimeError('Android input notification permission was not confirmed')
    current = settings(adb,'get','enabled_accessibility_services')
    services = components(current)
    if not has_component(services,ATTENTION):
        settings(adb,'put','enabled_accessibility_services',':'.join([*services,ATTENTION]))
        if not has_component(components(settings(adb,'get','enabled_accessibility_services')),ATTENTION):
            raise RuntimeError('Android input accessibility permission was not confirmed')
    enable_setting(adb,'accessibility_enabled')


def install_once(adb, root, serial, path, apk):
    if apk is None:
        apk = command([sys.executable], [str(root/'scripts/prebuilt_phone.py'),'--root',str(root),'--app','input'], 'APK preparation', timeout=None)
    if not Path(apk).is_file():raise RuntimeError('Android input APK is unavailable')
    # This durable record precedes the only installation attempt. A crash here is
    # deliberately unknown, including a crash immediately before invoking ADB.
    save_pending(path,serial,'install_unknown')
    try:command(adb, ['install','--no-incremental','-r','--user',USER,str(apk)], 'install', timeout=60)
    except RuntimeError:pass
    if not package_present(adb):
        raise RuntimeError('Android input install outcome is unknown; package absent, not retried')


def setup_input(executable, serial, root, *, apk=None, clock=time.monotonic, pause=time.sleep):
    root = Path(root).resolve()
    path = root/'data/android/input-setup.pending.json'
    if path.parent.is_symlink():raise RuntimeError('Android input pending owner is invalid')
    path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    # Services acquire their orchestration lock first; direct CLI callers acquire
    # only this lock. The input owner alone reads and changes the pending intent.
    try:lock = acquire(path.parent/'input-setup.lock')
    except BlockingIOError:
        raise RuntimeError('Android input setup already in progress') from None
    with lock:
        return _setup_locked(executable,serial,root,path,apk,clock,pause)


def _setup_locked(executable, serial, root, path, apk, clock, pause):
    pending = pending_record(path,serial)
    adb = [str(executable),'-P','15037','-s',serial]
    present = package_present(adb)
    if present and pending is None:return 'skipped'
    if pending is not None and not present:
        raise RuntimeError('Android input install outcome is unknown; package absent, not retried')
    if command(adb,['shell','am','get-current-user'],'user') != USER:
        raise RuntimeError('Android input setup requires user 0')
    if not present:install_once(adb,root,serial,path,apk)
    if pending is None or pending['stage'] != 'configure':save_pending(path,serial,'configure')
    await_registration(adb,clock,pause)
    configure_ime(adb)
    configure_services(adb)
    path.unlink()
    descriptor = os.open(path.parent,os.O_RDONLY)
    try:os.fsync(descriptor)
    finally:os.close(descriptor)
    return 'configured'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb',type=Path,required=True)
    parser.add_argument('--serial',required=True)
    parser.add_argument('--root',type=Path,required=True)
    args = parser.parse_args()
    try:setup_input(args.adb,args.serial,args.root)
    except RuntimeError as error:raise SystemExit(str(error)) from None
    except (OSError,ValueError):raise SystemExit('Android input setup state could not be confirmed') from None


if __name__ == '__main__':main()
