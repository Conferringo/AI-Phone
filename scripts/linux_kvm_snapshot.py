"""Read-only observations of the fixed KVM node, never an admission policy."""
import errno
import json
import os
import platform
import stat


DEVICE = '/dev/kvm'
PHASES = ('preflight_ready', 'engine_ready', 'before_android_start')


def error_kind(error):
    return {
        errno.EACCES: 'permission_denied', errno.EPERM: 'permission_denied',
        errno.ENOENT: 'missing', errno.ENODEV: 'unavailable', errno.ENXIO: 'unavailable',
        errno.ELOOP: 'symlink', errno.ENOTTY: 'unsupported', errno.EINVAL: 'unsupported',
    }.get(getattr(error, 'errno', None), 'unknown')


def device_identity(info):
    return info.st_dev, info.st_ino, info.st_rdev, info.st_mode, info.st_uid, info.st_gid


def access_bits(effective):
    result = {'read': None, 'write': None}
    for key, mode in (('read', os.R_OK), ('write', os.W_OK)):
        try:
            result[key] = os.access(DEVICE, mode, effective_ids=effective, follow_symlinks=False)
        except (OSError, NotImplementedError, TypeError):
            pass
    return result


def api_version(info):
    result = {'status': 'unknown', 'version': None, 'error': None}
    try:
        import fcntl
        descriptor = os.open(DEVICE, os.O_RDWR | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
        try:
            opened = os.fstat(descriptor)
            if not stat.S_ISCHR(opened.st_mode) or device_identity(opened) != device_identity(info):
                return {**result, 'error': 'changed'}
            version = fcntl.ioctl(descriptor, 0xAE00, 0)  # KVM_GET_API_VERSION; never KVM_CREATE_VM.
            if type(version) is int and 0 <= version <= 65535:
                return {'status': 'ready' if version == 12 else 'api_mismatch', 'version': version, 'error': None}
        finally:
            os.close(descriptor)
    except OSError as error:
        result['error'] = error_kind(error)
    except (ImportError, AttributeError, ValueError, TypeError):
        pass
    return result


def linux_snapshot():
    result = {
        'platform': 'linux',
        'override_present': 'ANDROID_EMULATOR_KVM_DEVICE' in os.environ,
        'identity': {'uid': os.getuid(), 'euid': os.geteuid()},
        'device': {'kind': 'unknown', 'mode': None, 'uid': None, 'gid': None,
                   'dev': None, 'inode': None, 'rdev': None, 'error': None},
        'access': {'real': {'read': None, 'write': None}, 'effective': {'read': None, 'write': None}},
        'ioctl': {'status': 'not_probed', 'version': None, 'error': None},
    }
    try:
        info = os.lstat(DEVICE)
        kind = 'character' if stat.S_ISCHR(info.st_mode) else 'symlink' if stat.S_ISLNK(info.st_mode) else 'other'
        result['device'].update(kind=kind, mode=stat.S_IMODE(info.st_mode), uid=info.st_uid, gid=info.st_gid,
                                dev=info.st_dev, inode=info.st_ino, rdev=info.st_rdev)
        if kind == 'character':
            result['access'] = {'real': access_bits(False), 'effective': access_bits(True)}
            result['ioctl'] = api_version(info)
            if device_identity(os.lstat(DEVICE)) != device_identity(info):
                result['device']['error'] = 'changed'
    except OSError as error:
        result['device']['error'] = error_kind(error)
    except (AttributeError, ValueError, TypeError):
        result['device']['error'] = 'unknown'
    return result


def snapshot():
    try:
        return linux_snapshot() if platform.system() == 'Linux' else {'platform': 'unsupported'}
    except (OSError, AttributeError, ValueError, TypeError):
        return {'platform': 'unknown'}


def emit_snapshot(phase):
    if phase not in PHASES:
        raise ValueError('Invalid KVM observation phase')
    try:
        value = snapshot()
        if value['platform'] == 'unsupported':
            return
        print('AI_PHONE_KVM ' + json.dumps({'format': 1, 'phase': phase, 'kvm': value}, sort_keys=True), flush=True)
    except (OSError, AttributeError, ValueError, TypeError):
        pass  # Optional diagnostic output cannot change the installer's decision.
