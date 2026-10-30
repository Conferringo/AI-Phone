"""Native ACE semantics, independent of SDDL spelling and host integer widths."""
import ctypes as c
import struct
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SYSTEM='S-1-5-18'
USER='S-1-5-21-123-456-789-1001'
FULL=0x001f01ff


def sid_bytes(value):
    parts=value.split('-');authority=int(parts[2]);sub=[int(v) for v in parts[3:]]
    return bytes([1,len(sub)])+authority.to_bytes(6,'big')+b''.join(v.to_bytes(4,'little') for v in sub)


def pointer_value(value):
    return value.value if isinstance(value,c.c_void_p) else value


class NativeAclFixture:
    def __init__(self,entries=None,control=0x9404,present=True,user=USER):
        self.user=user;self.control=control;self.present=present;self.allocated={};self.references=[];self.buffers=[]
        for sid,mask,flags,kind in entries if entries is not None else [(SYSTEM,FULL,0,0),(USER,FULL,0,0)]:
            body=sid_bytes(sid);self.buffers.append(c.create_string_buffer(struct.pack('<BBHI',kind,flags,8+len(body),mask)+body))
        self.security=SimpleNamespace(GetNamedSecurityInfoW=self.descriptor,GetSecurityDescriptorControl=self.flags,GetSecurityDescriptorDacl=self.dacl,GetAclInformation=self.information,GetAce=self.ace,ConvertStringSidToSidW=self.convert_sid,EqualSid=self.equal,IsValidSid=self.valid)
        self.kernel=SimpleNamespace(LocalFree=self.free)

    def descriptor(self,*args):
        if args[1]!=1:raise AssertionError('Expected SE_FILE_OBJECT')
        c.cast(args[-1],c.POINTER(c.c_void_p))[0]=123;self.allocated[123]='descriptor';return 0
    def flags(self,value,control,revision):
        c.cast(control,c.POINTER(c.c_uint16))[0]=self.control;c.cast(revision,c.POINTER(c.c_uint32))[0]=1;return True
    def dacl(self,value,present,acl,defaulted):
        c.cast(present,c.POINTER(c.c_int32))[0]=self.present;c.cast(acl,c.POINTER(c.c_void_p))[0]=789 if self.present else None;return True
    def information(self,acl,output,size,kind):
        if size!=12 or kind!=2:raise AssertionError('Wrong ACL_SIZE_INFORMATION ABI')
        values=c.cast(output,c.POINTER(c.c_uint32));values[0]=len(self.buffers);values[1]=8+sum(c.sizeof(v)-1 for v in self.buffers);values[2]=0;return True
    def ace(self,acl,index,output):
        c.cast(output,c.POINTER(c.c_void_p))[0]=c.addressof(self.buffers[index]);return True
    def convert_sid(self,value,output):
        buffer=c.create_string_buffer(sid_bytes(value));self.references.append(buffer);address=c.addressof(buffer);self.allocated[address]='sid';c.cast(output,c.POINTER(c.c_void_p))[0]=address;return True
    def valid(self,value):
        address=pointer_value(value);return c.c_uint8.from_address(address).value==1 and c.c_uint8.from_address(address+1).value<=15
    def equal(self,left,right):
        def body(value):
            address=pointer_value(value);length=8+4*c.c_uint8.from_address(address+1).value;return c.string_at(address,length)
        return body(left)==body(right)
    def free(self,value):
        del self.allocated[pointer_value(value)]
    def run(self):
        import windows_private
        with patch('windows_private.api',return_value=(self.security,self.kernel)),patch('windows_private.user_sid',return_value=self.user):
            result=windows_private.is_private('fixture')
        if self.allocated:raise AssertionError('Native descriptor/SID allocation leaked')
        return result


class NativeAclPolicyTests(unittest.TestCase):
    def test_private_accounts_match_by_sid_with_order_and_object_inheritance_variations(self):
        for flags in (0,3,0x13):
            with self.subTest(flags=flags):self.assertTrue(NativeAclFixture([(USER,FULL,flags,0),(SYSTEM,FULL,flags,0)]).run())

    def test_extra_or_wrong_accounts_and_nonapplying_or_incomplete_grants_are_rejected(self):
        bad=[[(SYSTEM,FULL,0,0),(USER,FULL,0,0),('S-1-1-0',FULL,0,0)],[(SYSTEM,FULL,0,0),('S-1-1-0',FULL,0,0)],[(USER,FULL,0,0),(USER,FULL,0,0)],[(SYSTEM,FULL,0,0),(USER,0x120089,0,0)],[(SYSTEM,FULL,0,0),(USER,FULL,8,0)],[(SYSTEM,FULL,0,0),(USER,FULL,0,1)],[(SYSTEM,FULL,0,0),(USER,FULL,0x80,0)]]
        for entries in bad:
            with self.subTest(entries=entries):self.assertFalse(NativeAclFixture(entries).run())
        for fixture in (NativeAclFixture(control=0x8004),NativeAclFixture(control=0x9000),NativeAclFixture(present=False)):
            self.assertFalse(fixture.run())

    def test_malformed_sid_lengths_are_rejected_before_native_comparison(self):
        fixture=NativeAclFixture();buffer=fixture.buffers[1];c.c_uint8.from_buffer(buffer,9).value=15
        original=fixture.security.IsValidSid;target=c.addressof(buffer)+8
        def valid(address):
            self.assertNotEqual(pointer_value(address),target,'Malformed SID reached native validation');return original(address)
        fixture.security.IsValidSid=valid;self.assertFalse(fixture.run())
        fixture=NativeAclFixture();c.c_uint16.from_buffer(fixture.buffers[1],2).value=4
        self.assertFalse(fixture.run())

    def test_sid_and_ace_size_boundaries_are_explicit(self):
        from windows_private import applying_allow,AceHeader,private_dacl
        for size,valid in ((15,False),(16,True),(76,True),(77,False)):self.assertEqual(applying_allow(AceHeader(0,0,size)),valid)
        longest='S-1-5-'+'-'.join(str(n) for n in range(15))
        self.assertTrue(NativeAclFixture([(SYSTEM,FULL,0,0),(longest,FULL,0,0)],user=longest).run())
        with patch('windows_private.user_sid',return_value=USER):self.assertEqual(private_dacl(),'D:P(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;'+USER+')')
        fixture=NativeAclFixture();c.c_uint8.from_buffer(fixture.buffers[1],8).value=2;self.assertFalse(fixture.run())


if __name__=='__main__':unittest.main()
