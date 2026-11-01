"""Cross-platform discovery must skip POSIX fixtures before importing them."""
from pathlib import Path
import subprocess
import sys
import unittest


class KvmPlatformBoundaryTests(unittest.TestCase):
    def test_non_posix_discovery_skips_before_any_posix_import_or_device_access(self):
        code = '''import builtins,os,sys,unittest
with open(sys.argv[1]) as stream:source=stream.read()
original_import=builtins.__import__
def guarded_import(name,*args,**kwargs):
 if name in ('fcntl','android_boot_diagnostics','linux_kvm_snapshot','linux_host'):
  raise AssertionError('POSIX dependency imported before platform skip: '+name)
 return original_import(name,*args,**kwargs)
def forbidden(*args,**kwargs):raise AssertionError('No device/process identity access before skip')
builtins.__import__=guarded_import
for name in ('lstat','access','open','getuid','geteuid'):setattr(os,name,forbidden)
os.name='nt'
try:exec(compile(source,sys.argv[1],'exec'),{'__name__':'platform_fixture','__file__':sys.argv[1]})
except unittest.SkipTest as error:
 assert str(error)=='POSIX fixed-device diagnostic fixtures',str(error)
 print('EXPLICIT_NON_POSIX_SKIP')
else:raise AssertionError('POSIX fixtures were not explicitly skipped')
'''
        source = Path(__file__).with_name('test_linux_kvm_snapshot.py')
        result = subprocess.run([sys.executable, '-c', code, str(source)], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.strip(), 'EXPLICIT_NON_POSIX_SKIP')


if __name__ == '__main__':
    unittest.main()
