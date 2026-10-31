"""Read-only observations of the fixed KVM node, never an admission policy."""
import errno
import json
import os
import platform
import stat
import struct


DEVICE = '/dev/kvm'
PHASES = ('preflight_ready', 'toolchain_ready', 'sdk_tools_ready', 'engine_ready', 'before_android_start')
MAX_ACL_BYTES = 4096  # Linux bounds getxattr itself to 64 KiB; parse a smaller value only.
MAX_GROUPS = 65536  # Linux UAPI NGROUPS_MAX; never emit the process group list.


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


def acl_state(status='unknown', error=None, user=None, mask=None):
    return {'status': status, 'euid_permissions': user, 'mask_permissions': mask, 'error': error}


def acl_payload_error(body):
    # Linux UAPI posix_acl_xattr: LE32 version 2, then LE16 tag/perms + LE32 ID.
    if not isinstance(body, bytes) or len(body) > MAX_ACL_BYTES:
        return 'size_limit'
    if len(body) < 4 or (len(body) - 4) % 8:
        return 'malformed'
    if struct.unpack_from('<I', body)[0] != 2:
        return 'unsupported'
    return None


def valid_acl_entry(tag, permissions, identity, seen):
    if tag not in (1, 2, 4, 8, 16, 32) or permissions > 7 or (tag, identity) in seen:
        return False
    if tag in (2, 8):
        return identity != 0xffffffff
    return identity == 0xffffffff


def project_acl(entries, uid):
    seen, user, mask, named = set(), None, None, False
    for tag, permissions, identity in entries:
        if not valid_acl_entry(tag, permissions, identity, seen):
            return acl_state(error='malformed')
        seen.add((tag, identity))
        named |= tag in (2, 8)
        if tag == 2 and identity == uid:
            user = permissions
        elif tag == 16:
            mask = permissions
    required = {(1, 0xffffffff), (4, 0xffffffff), (32, 0xffffffff)}
    if not required <= seen or (named and mask is None):
        return acl_state(error='malformed')
    return acl_state('present', user=user, mask=mask)


def decode_acl(body, uid):
    error = acl_payload_error(body)
    if error:
        return acl_state(error=error)
    return project_acl(struct.iter_unpack('<HHI', body[4:]), uid)


def access_acl(uid):
    try:
        body = os.getxattr(DEVICE, 'system.posix_acl_access', follow_symlinks=False)
        return decode_acl(body, uid)
    except OSError as error:
        if error.errno == errno.ENODATA:
            return acl_state('absent')
        kind = 'unsupported' if error.errno in (errno.ENOTSUP, errno.EOPNOTSUPP) else error_kind(error)
        return acl_state(error=kind)
    except (AttributeError, NotImplementedError, ValueError, TypeError):
        return acl_state(error='unsupported')


def node_group_member(gid):
    try:
        if gid == os.getegid():
            return True
        groups = os.getgroups()
        return gid in groups if len(groups) <= MAX_GROUPS else None
    except (OSError, AttributeError, NotImplementedError, ValueError, TypeError):
        return None


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
        'acl': acl_state('not_probed'),
        'node_group_member': None,
        'ioctl': {'status': 'not_probed', 'version': None, 'error': None},
    }
    try:
        info = os.lstat(DEVICE)
        kind = 'character' if stat.S_ISCHR(info.st_mode) else 'symlink' if stat.S_ISLNK(info.st_mode) else 'other'
        result['device'].update(kind=kind, mode=stat.S_IMODE(info.st_mode), uid=info.st_uid, gid=info.st_gid,
                                dev=info.st_dev, inode=info.st_ino, rdev=info.st_rdev)
        if kind == 'character':
            result['access'] = {'real': access_bits(False), 'effective': access_bits(True)}
            result['acl'] = access_acl(result['identity']['euid'])
            result['node_group_member'] = node_group_member(info.st_gid)
            result['ioctl'] = api_version(info)
            if device_identity(os.lstat(DEVICE)) != device_identity(info):
                result['device']['error'] = 'changed'
    except OSError as error:
        result['device']['error'] = error_kind(error)
    except (AttributeError, ValueError, TypeError):
        result['device']['error'] = 'unknown'
    if result['device']['error'] and result['acl']['status'] != 'not_probed':
        result['acl'] = acl_state(error=result['device']['error'])
        result['node_group_member'] = None
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
