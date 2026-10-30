"""Windows security API error/ownership contracts; real ACL tests run on Windows."""
from contextlib import contextmanager
import ctypes as c
from ctypes import wintypes as w
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

class WindowsSecurityTests(unittest.TestCase):
    def test_sized_security_queries_refuse_invalid_lengths_and_propagate_errors(self):
        from windows_private import query_buffer
        for size in (0,65537):
            def invalid(*args):c.cast(args[-1],c.POINTER(w.DWORD))[0]=size;return False
            with patch('ctypes.get_last_error',return_value=122,create=True):
                with self.assertRaises(ValueError):query_buffer(invalid,'fixture')
        def valid(*args):
            pointer,length,result=args[-3:];self.assertEqual(length,8 if pointer is not None else 0);c.cast(result,c.POINTER(w.DWORD))[0]=8
            if pointer is not None:c.memmove(pointer,b'fixture\0',8);return True
            return False
        with patch('ctypes.get_last_error',return_value=122,create=True):self.assertEqual(query_buffer(valid).raw,b'fixture\0')
        with patch('ctypes.get_last_error',return_value=5,create=True),patch('ctypes.WinError',side_effect=lambda error:OSError(error,'fixture'),create=True):
            with self.assertRaises(OSError):query_buffer(lambda *args:False)

    def test_descriptor_and_token_owners_release_native_handles_on_failure(self):
        from windows_private import protect,token
        released=[]
        def allocate(sddl,revision,output,size):c.cast(output,c.POINTER(c.c_void_p))[0]=123;return True
        def open_token(process,access,output):c.cast(output,c.POINTER(w.HANDLE))[0]=456;return True
        def get_dacl(descriptor,present,acl,defaulted):
            c.cast(present,c.POINTER(w.BOOL))[0]=True;c.cast(acl,c.POINTER(c.c_void_p))[0]=789;return True
        security=SimpleNamespace(ConvertStringSecurityDescriptorToSecurityDescriptorW=allocate,GetSecurityDescriptorDacl=get_dacl,SetNamedSecurityInfoW=lambda *args:5,OpenProcessToken=open_token)
        kernel=SimpleNamespace(LocalFree=lambda pointer:released.append(pointer.value),GetCurrentProcess=lambda:1,CloseHandle=lambda handle:released.append(handle.value) or True)
        with patch('windows_private.api',return_value=(security,kernel)),patch('windows_private.private_dacl',return_value='fixture'),patch('ctypes.get_last_error',return_value=5,create=True),patch('ctypes.WinError',side_effect=lambda error:OSError(error,'fixture'),create=True):
            with self.assertRaises(OSError):protect(Path('fixture'))
            with self.assertRaises(RuntimeError):
                with token():raise RuntimeError('cancelled')
        self.assertEqual(released,[123,456])

    def test_sid_and_acl_strings_are_released_and_only_exact_private_acl_is_accepted(self):
        import windows_private
        values=[];released=[];sid='S-1-5-21-123-456-789-500'
        def text_result(text,output):
            buffer=c.create_unicode_buffer(text);values.append(buffer);c.cast(output,c.POINTER(w.LPWSTR))[0]=c.cast(buffer,w.LPWSTR);return True
        pointer=c.c_void_p(42)
        def get_descriptor(*args):
            self.assertEqual(args[1],1);c.cast(args[-1],c.POINTER(c.c_void_p))[0]=123;return 0
        canonical='D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;LA)';actual=[canonical]
        def converted(text,revision,out,size):c.cast(out,c.POINTER(c.c_void_p))[0]=456;return True
        def formatted(raw,revision,info,out,length):
            if revision!=1:return False
            return text_result(canonical if raw.value==456 else actual[0],out)
        security=SimpleNamespace(GetTokenInformation=object(),GetNamedSecurityInfoW=get_descriptor,ConvertSidToStringSidW=lambda raw,out:text_result(sid,out) if raw==42 else False,ConvertStringSecurityDescriptorToSecurityDescriptorW=converted,ConvertSecurityDescriptorToStringSecurityDescriptorW=formatted)
        kernel=SimpleNamespace(LocalFree=lambda value:released.append(value.value))
        @contextmanager
        def fixture_token():yield 1
        windows_private.user_sid.cache_clear()
        try:
            with patch('windows_private.api',return_value=(security,kernel)),patch('windows_private.token',fixture_token),patch('windows_private.query_buffer',side_effect=lambda function,handle,kind:c.string_at(c.byref(pointer),c.sizeof(pointer))+b'\0'*c.sizeof(pointer) if kind==1 else (_ for _ in ()).throw(OSError('wrong token class'))):
                self.assertEqual(windows_private.user_sid(),sid);self.assertTrue(windows_private.is_private('fixture'))
                actual[0]='D:PAI'+canonical[3:]
                self.assertTrue(windows_private.is_private('fixture'))
                actual[0]='D:(A;;FA;;;WD)'
                self.assertFalse(windows_private.is_private('fixture'))
            self.assertEqual(released.count(123),3);self.assertEqual(released.count(456),3);self.assertEqual(len(released),13)
        finally:windows_private.user_sid.cache_clear()

    def test_publication_requests_replace_and_write_through_and_propagates_failure(self):
        from windows_private import replace
        calls=[];kernel=SimpleNamespace(MoveFileExW=lambda *args:calls.append(args) or True)
        with patch('windows_private.api',return_value=(None,kernel)):replace('source','target')
        self.assertEqual(calls,[('source','target',9)])
        kernel.MoveFileExW=lambda *args:False
        with patch('windows_private.api',return_value=(None,kernel)),patch('ctypes.get_last_error',return_value=5,create=True),patch('ctypes.WinError',side_effect=lambda error:OSError(error,'fixture'),create=True):
            with self.assertRaises(OSError):replace('source','target')

    def test_exact_security_buffer_limits_and_missing_dacl_are_validated(self):
        from windows_private import query_buffer,protect
        for requested in (1,65536):
            def query(*args):
                out,length,size=args[-3:];c.cast(size,c.POINTER(w.DWORD))[0]=requested
                return out is not None
            with patch('ctypes.get_last_error',return_value=122,create=True):self.assertEqual(len(query_buffer(query)),requested)
        def allocate(sddl,revision,output,size):
            self.assertEqual(revision,1);c.cast(output,c.POINTER(c.c_void_p))[0]=123;return True
        released=[];seen=[]
        def set_acl(path,kind,info,owner,group,acl,sacl):
            seen.append((kind,info,owner,group,acl.value,sacl));return 0
        for present_value,acl_value in ((True,789),(False,789),(True,None)):
            def get_acl(descriptor,present,acl,defaulted):
                c.cast(present,c.POINTER(w.BOOL))[0]=present_value;c.cast(acl,c.POINTER(c.c_void_p))[0]=acl_value;return True
            security=SimpleNamespace(ConvertStringSecurityDescriptorToSecurityDescriptorW=allocate,GetSecurityDescriptorDacl=get_acl,SetNamedSecurityInfoW=set_acl)
            with patch('windows_private.api',return_value=(security,SimpleNamespace(LocalFree=lambda value:released.append(value.value)))),patch('windows_private.private_dacl',return_value='fixture'):
                if present_value and acl_value:protect('fixture')
                else:
                    with self.assertRaises(ValueError):protect('fixture')
        self.assertEqual(seen,[(1,0x80000004,None,None,789,None)]);self.assertEqual(released,[123]*3)


if __name__=='__main__':unittest.main()
