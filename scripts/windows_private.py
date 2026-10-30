"""Current-user/SYSTEM ACLs and durable same-volume publication on Windows."""
from contextlib import contextmanager
from functools import lru_cache

DACL = 4
PROTECTED_DACL = 0x80000000


@lru_cache(maxsize=1)
def api():
    import ctypes as c
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
        (security.ConvertSecurityDescriptorToStringSecurityDescriptorW, [c.c_void_p, w.DWORD, w.DWORD, c.POINTER(w.LPWSTR), c.POINTER(w.DWORD)], w.BOOL),
    ]
    for function, arguments, result in definitions:
        function.argtypes, function.restype = arguments, result
    return security, kernel


def checked(result):
    if not result:
        import ctypes
        raise ctypes.WinError(ctypes.get_last_error())


def checked_code(code):
    if code:
        import ctypes
        raise ctypes.WinError(code)


def query_buffer(function, *arguments):
    import ctypes as c
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
    import ctypes as c
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
    import ctypes as c
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


def private_dacl():
    return 'D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;' + user_sid() + ')'


@contextmanager
def constructed(value):
    import ctypes as c
    security, kernel = api()
    result = c.c_void_p()
    checked(security.ConvertStringSecurityDescriptorToSecurityDescriptorW(value, 1, c.byref(result), None))
    try:
        yield result
    finally:
        kernel.LocalFree(result)


def protect(path):
    import ctypes as c
    from ctypes import wintypes as w
    security, _ = api()
    with constructed(private_dacl()) as value:
        present, defaulted, acl = w.BOOL(), w.BOOL(), c.c_void_p()
        checked(security.GetSecurityDescriptorDacl(value, c.byref(present), c.byref(acl), c.byref(defaulted)))
        if not present.value or not acl.value:
            raise ValueError('Private Windows DACL is missing')
        checked_code(security.SetNamedSecurityInfoW(str(path), 1, DACL | PROTECTED_DACL, None, None, acl, None))


@contextmanager
def descriptor(path):
    import ctypes as c
    security, kernel = api()
    value = c.c_void_p()
    checked_code(security.GetNamedSecurityInfoW(str(path), 1, DACL, None, None, None, None, c.byref(value)))
    try:
        yield value
    finally:
        kernel.LocalFree(value)


def descriptor_text(value):
    import ctypes as c
    from ctypes import wintypes as w
    security, kernel = api()
    text = w.LPWSTR()
    checked(security.ConvertSecurityDescriptorToStringSecurityDescriptorW(value, 1, DACL, c.byref(text), None))
    try:
        return text.value
    finally:
        kernel.LocalFree(c.cast(text, c.c_void_p))


def is_private(path):
    with descriptor(path) as value:
        actual = descriptor_text(value)
    # Let Windows normalize SID spellings (including built-in-account aliases).
    with constructed(private_dacl()) as value:
        expected = descriptor_text(value)
    # AUTO_INHERITED is descriptive; PROTECTED and both exact ACEs remain mandatory.
    return actual in (expected, 'D:PAI' + expected[3:])


def replace(source, destination):
    _, kernel = api()
    # Same-directory temporary files stay on one volume; include WRITE_THROUGH.
    checked(kernel.MoveFileExW(str(source), str(destination), 1 | 8))
