"""Current-user/SYSTEM ACLs and durable same-volume publication on Windows."""
from contextlib import contextmanager, ExitStack
import ctypes as c
from functools import lru_cache

DACL = 4
PROTECTED_DACL = 0x80000000
FULL_ACCESS = 0x001f01ff


class AclSizeInfo(c.Structure):
    _fields_ = [('count', c.c_uint32), ('used', c.c_uint32), ('free', c.c_uint32)]


class AceHeader(c.Structure):
    _fields_ = [('kind', c.c_uint8), ('flags', c.c_uint8), ('size', c.c_uint16)]


class AllowedAce(c.Structure):
    _fields_ = [('header', AceHeader), ('mask', c.c_uint32), ('sid_start', c.c_uint32)]


@lru_cache(maxsize=1)
def api():
    from ctypes import wintypes as w
    security = c.WinDLL('advapi32', use_last_error=True)
    kernel = c.WinDLL('kernel32', use_last_error=True)
    definitions = [
        (kernel.GetCurrentProcess, [], w.HANDLE),
        (kernel.CloseHandle, [w.HANDLE], w.BOOL),
        (kernel.LocalFree, [c.c_void_p], c.c_void_p),
        (kernel.MoveFileExW, [w.LPCWSTR, w.LPCWSTR, w.DWORD], w.BOOL),
        (security.OpenProcessToken, [w.HANDLE, w.DWORD, c.POINTER(w.HANDLE)], w.BOOL),
        (security.GetTokenInformation, [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD)], w.BOOL),
        (security.ConvertSidToStringSidW, [c.c_void_p, c.POINTER(w.LPWSTR)], w.BOOL),
        (security.ConvertStringSecurityDescriptorToSecurityDescriptorW, [w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.POINTER(w.DWORD)], w.BOOL),
        (security.GetSecurityDescriptorDacl, [c.c_void_p, c.POINTER(w.BOOL), c.POINTER(c.c_void_p), c.POINTER(w.BOOL)], w.BOOL),
        (security.SetNamedSecurityInfoW, [w.LPWSTR, c.c_int, w.DWORD, c.c_void_p, c.c_void_p, c.c_void_p, c.c_void_p], w.DWORD),
        (security.GetNamedSecurityInfoW, [w.LPCWSTR, c.c_int, w.DWORD, c.c_void_p, c.c_void_p, c.c_void_p, c.c_void_p, c.POINTER(c.c_void_p)], w.DWORD),
        (security.GetSecurityDescriptorControl, [c.c_void_p, c.POINTER(c.c_uint16), c.POINTER(c.c_uint32)], w.BOOL),
        (security.GetAclInformation, [c.c_void_p, c.c_void_p, w.DWORD, c.c_int], w.BOOL),
        (security.GetAce, [c.c_void_p, w.DWORD, c.POINTER(c.c_void_p)], w.BOOL),
        (security.IsValidSid, [c.c_void_p], w.BOOL),
        (security.EqualSid, [c.c_void_p, c.c_void_p], w.BOOL),
        (security.ConvertStringSidToSidW, [w.LPCWSTR, c.POINTER(c.c_void_p)], w.BOOL),
    ]
    for function, arguments, result in definitions:
        function.argtypes, function.restype = arguments, result
    return security, kernel


def checked(result):
    if not result:
        raise c.WinError(c.get_last_error())


def checked_code(code):
    if code:
        raise c.WinError(code)


def query_buffer(function, *arguments):
    from ctypes import wintypes as w
    size = w.DWORD()
    result = function(*arguments, None, 0, c.byref(size))
    if not result and c.get_last_error() != 122:
        checked(result)
    if not 0 < size.value <= 65536:
        raise ValueError('Invalid Windows security descriptor size')
    buffer = c.create_string_buffer(size.value)
    checked(function(*arguments, buffer, size.value, c.byref(size)))
    return buffer


@contextmanager
def token():
    from ctypes import wintypes as w
    security, kernel = api()
    handle = w.HANDLE()
    checked(security.OpenProcessToken(kernel.GetCurrentProcess(), 8, c.byref(handle)))
    try:
        yield handle
    finally:
        checked(kernel.CloseHandle(handle))


@lru_cache(maxsize=1)
def user_sid():
    from ctypes import wintypes as w
    security, kernel = api()
    with token() as handle:
        buffer = query_buffer(security.GetTokenInformation, handle, 1)
        sid = c.cast(buffer, c.POINTER(c.c_void_p))[0]
        text = w.LPWSTR()
        checked(security.ConvertSidToStringSidW(sid, c.byref(text)))
        try:
            return text.value
        finally:
            kernel.LocalFree(c.cast(text, c.c_void_p))


def private_sids():
    return ('S-1-5-18', user_sid())


def private_dacl():
    return 'D:P' + ''.join('(A;OICI;FA;;;' + sid + ')' for sid in private_sids())


@contextmanager
def constructed(value):
    security, kernel = api()
    result = c.c_void_p()
    checked(security.ConvertStringSecurityDescriptorToSecurityDescriptorW(value, 1, c.byref(result), None))
    try:
        yield result
    finally:
        kernel.LocalFree(result)


def protect(path):
    security, _ = api()
    with constructed(private_dacl()) as value:
        acl = get_dacl(value)
        if acl is None:
            raise ValueError('Private Windows DACL is missing')
        checked_code(security.SetNamedSecurityInfoW(str(path), 1, DACL | PROTECTED_DACL, None, None, acl, None))


@contextmanager
def descriptor(path):
    security, kernel = api()
    value = c.c_void_p()
    checked_code(security.GetNamedSecurityInfoW(str(path), 1, DACL, None, None, None, None, c.byref(value)))
    try:
        yield value
    finally:
        kernel.LocalFree(value)


def protected(value):
    security, _ = api()
    control, revision = c.c_uint16(), c.c_uint32()
    checked(security.GetSecurityDescriptorControl(value, c.byref(control), c.byref(revision)))
    return control.value & 0x1004 == 0x1004


def get_dacl(value):
    from ctypes import wintypes as w
    security, _ = api()
    present, defaulted, acl = w.BOOL(), w.BOOL(), c.c_void_p()
    checked(security.GetSecurityDescriptorDacl(value, c.byref(present), c.byref(acl), c.byref(defaulted)))
    return acl if present.value and acl.value else None


def ace_count(acl):
    security, _ = api()
    info = AclSizeInfo()
    checked(security.GetAclInformation(acl, c.byref(info), c.sizeof(info), 2))
    return info.count


def applying_allow(header):
    # Origin/child-inheritance flags do not change an ACE's grant on this object.
    # INHERIT_ONLY, audit flags and unknown bits are not applicable grants.
    return header.kind == 0 and not header.flags & ~0x17 and 16 <= header.size <= 76


def ace_sid(acl, index):
    security, _ = api()
    pointer = c.c_void_p()
    checked(security.GetAce(acl, index, c.byref(pointer)))
    if not pointer.value:
        return None
    header = c.cast(pointer, c.POINTER(AceHeader)).contents
    if not applying_allow(header):
        return None
    value = c.cast(pointer, c.POINTER(AllowedAce)).contents
    if value.mask != FULL_ACCESS:
        return None
    sid = pointer.value + AllowedAce.sid_start.offset
    count = c.c_uint8.from_address(sid + 1).value
    if count > 15 or header.size != 16 + 4 * count:
        return None
    return c.c_void_p(sid) if security.IsValidSid(sid) else None


@contextmanager
def sid_pointer(value):
    security, kernel = api()
    pointer = c.c_void_p()
    checked(security.ConvertStringSidToSidW(value, c.byref(pointer)))
    try:
        yield pointer
    finally:
        kernel.LocalFree(pointer)


def consume_sid(value, remaining):
    security, _ = api()
    for index, candidate in enumerate(remaining):
        if security.EqualSid(value, candidate):
            remaining.pop(index)
            return True
    return False


def is_private(path):
    with descriptor(path) as value:
        if not protected(value):
            return False
        acl = get_dacl(value)
        if acl is None or ace_count(acl) != 2:
            return False
        with ExitStack() as owners:
            remaining = [owners.enter_context(sid_pointer(sid)) for sid in private_sids()]
            for index in range(2):
                sid = ace_sid(acl, index)
                if sid is None or not consume_sid(sid, remaining):
                    return False
            return True


def replace(source, destination):
    _, kernel = api()
    # Same-directory temporary files stay on one volume; include WRITE_THROUGH.
    checked(kernel.MoveFileExW(str(source), str(destination), 1 | 8))
