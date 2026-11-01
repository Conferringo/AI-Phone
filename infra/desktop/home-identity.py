"""Match the container account to its mounted home without migrating that home."""
import grp
import os
from pathlib import Path
import pwd
import stat
import subprocess
import tempfile

HOME = Path('/home/ai')
RUNTIME = Path('/run')


def command(argv, timeout=15):
    subprocess.run(argv, check=True, stdin=subprocess.DEVNULL,
                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=timeout)


def directory_identity(path):
    value = path.lstat()
    if not stat.S_ISDIR(value.st_mode):
        raise ValueError('Desktop identity path is not a directory')
    return value.st_dev, value.st_ino, value.st_uid, value.st_gid, stat.S_IMODE(value.st_mode)


def temporary_home(runtime):
    parent = directory_identity(runtime)
    if parent[2] != 0 or parent[4] & 0o022:
        raise ValueError('Desktop identity runtime directory is not privately controlled')
    folder = Path(tempfile.mkdtemp(prefix='.ai-phone-identity-', dir=runtime))
    identity = directory_identity(folder)
    if identity[2] != 0 or identity[4] != 0o700:
        raise ValueError('Desktop identity temporary directory is not owned')
    return folder, identity


def change_account(home, runtime, uid, gid):
    folder, identity = temporary_home(runtime)
    errors = []
    try:
        absent = folder / 'absent-home'
        try:
            absent.lstat()
        except FileNotFoundError:
            pass
        else:
            raise ValueError('Desktop identity temporary home is occupied')
        try:
            # shadow usermod otherwise walks an old OR new UID-owned home.
            # -d without -m selects this absent path for that automatic walk.
            command(['usermod', '--home', str(absent), '--uid', str(uid),
                     '--gid', str(gid), 'ai'])
        except BaseException as error:
            errors.append(error)
        finally:
            try:
                # No UID/GID flags: restore the fixed path without walking it.
                command(['usermod', '--home', str(home), 'ai'])
                if pwd.getpwnam('ai').pw_dir != str(home):
                    raise ValueError('Desktop account home restoration was not verified')
            except BaseException as error:
                errors.append(error)
    except BaseException as error:
        errors.append(error)
    finally:
        try:
            if directory_identity(folder) != identity:
                raise ValueError('Desktop identity temporary directory changed; preserved')
            # Never recursively delete: unexpected content is preserved as an error.
            folder.rmdir()
        except BaseException as error:
            errors.append(error)
    if errors:
        raise BaseExceptionGroup('Desktop account initialization failed', errors)


def desired_home_identity(home):
    owner = home.lstat()
    if not stat.S_ISDIR(owner.st_mode):
        raise ValueError('Desktop home is not a directory')
    legacy = owner.st_uid == 0
    uid, gid = (1000, 1000) if legacy else (owner.st_uid, owner.st_gid)
    if not 0 < uid < 2**31 or not 0 <= gid < 2**31:
        raise ValueError('Desktop home identity is unsupported')
    account = pwd.getpwnam('ai')
    if account.pw_dir != str(home):
        raise ValueError('Desktop account home differs from its fixed directory')
    try:
        occupied = pwd.getpwuid(uid)
    except KeyError:
        occupied = None
    if occupied is not None and occupied.pw_name != 'ai':
        raise ValueError('Desktop home UID belongs to another container account')
    return uid, gid, legacy, account


def ensure_primary_group(gid):
    try:
        grp.getgrgid(gid)
        missing_group = False
    except KeyError:
        missing_group = True
        name = 'ai-home-' + str(gid)
        try:
            grp.getgrnam(name)
        except KeyError:
            pass
        else:
            raise ValueError('Desktop home group name is occupied')
    if missing_group:
        # Reuse existing GIDs; never renumber another group or allow duplicates.
        command(['groupadd', '--gid', str(gid), 'ai-home-' + str(gid)])
        created = grp.getgrgid(gid)
        if created.gr_name != name:
            raise ValueError('Desktop home group creation was not verified')


def initialize(home=HOME, runtime=RUNTIME):
    if os.geteuid() != 0:
        raise ValueError('Desktop account initialization requires container root')
    uid, gid, legacy, account = desired_home_identity(home)
    ensure_primary_group(gid)
    if (account.pw_uid, account.pw_gid) != (uid, gid):
        change_account(home, runtime, uid, gid)
    current = pwd.getpwnam('ai')
    if (current.pw_uid, current.pw_gid, current.pw_dir) != (uid, gid, str(home)):
        raise ValueError('Desktop account identity was not verified')
    if legacy:
        # Retain the existing root-owned-home compatibility path only.
        command(['find', str(home), '(', '-type', 'd', '-o', '-type', 'f', ')',
                 '-exec', 'chown', str(uid) + ':' + str(gid), '{}', '+'], timeout=None)
    return uid, gid


if __name__ == '__main__':
    initialize()
