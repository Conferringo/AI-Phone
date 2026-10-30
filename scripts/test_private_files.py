"""Private file publication and installer locks have real process semantics."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

class PrivateFileTests(unittest.TestCase):
    @unittest.skipUnless(os.name=='nt','Windows native ACL semantics')
    def test_native_windows_acl_rejects_an_extra_world_read_grant(self):
        from private_files import atomic_json,is_private
        import windows_private
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'private'/'value.json';atomic_json(path,{'fixture':True})
            import ctypes as c
            from ctypes import wintypes as w
            security,_=windows_private.api();handle=w.HANDLE();c.set_last_error(0)
            result=security.OpenProcessToken(None,8,c.byref(handle))
            with self.assertRaises(OSError) as denied:windows_private.checked(result)
            self.assertEqual(denied.exception.winerror,6)
            c.set_last_error(0)
            with self.assertRaises(OSError) as missing:windows_private.replace(path.parent/'absent',path.parent/'destination')
            self.assertEqual(missing.exception.winerror,2)
            dacl=windows_private.private_dacl()+'(A;OICI;FR;;;WD)'
            with patch('windows_private.private_dacl',return_value=dacl):windows_private.protect(path)
            self.assertFalse(is_private(path));windows_private.protect(path);self.assertTrue(is_private(path))

    def test_private_descriptor_creation_failure_closes_its_owned_handle(self):
        from private_files import atomic_json
        from installer_lock import acquire
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);opened=[];original=os.open
            def remember(*args,**kwargs):
                descriptor=original(*args,**kwargs);opened.append(descriptor);return descriptor
            with patch('installer_lock.os.open',side_effect=remember),patch('installer_lock.protect',side_effect=OSError('fixture protection failure')):
                with self.assertRaises(OSError):acquire(root/'fixture.lock')
            with self.assertRaises(OSError):os.fstat(opened[-1])
            path=root/'state.json';atomic_json(path,{'retained':True})
            with patch('private_files.os.fdopen',side_effect=OSError('fixture descriptor failure')):
                with self.assertRaises(OSError):atomic_json(path,{'uncommitted':True})
            self.assertEqual(json.loads(path.read_text()),{'retained':True});self.assertEqual(list(root.glob('.credentials-*')),[])

    def test_unknown_aliases_are_preserved_without_touching_their_target(self):
        from private_files import atomic_json
        from installer_lock import acquire
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);target=root/'unrelated';target.write_text('keep');alias=root/'alias';alias.symlink_to(target)
            for action in (lambda:atomic_json(alias,{}),lambda:acquire(alias)):
                with self.assertRaises(ValueError):action()
            self.assertTrue(alias.is_symlink());self.assertEqual(target.read_text(),'keep')

    def test_private_atomic_json_replaces_and_preserves_the_old_file_on_write_failure(self):
        from private_files import atomic_json,is_private
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'owned'/'state.json';atomic_json(path,{'old':'retained'})
            self.assertTrue(is_private(path));self.assertTrue(is_private(path.parent));self.assertEqual(json.loads(path.read_text()),{'old':'retained'})
            with patch('private_files.json.dump',side_effect=OSError('fixture write failure')):
                with self.assertRaises(OSError):atomic_json(path,{'new':'not published'})
            self.assertEqual(json.loads(path.read_text()),{'old':'retained'});self.assertEqual(list(path.parent.glob('.credentials-*')),[])
            atomic_json(path,{'new':'committed'});self.assertEqual(json.loads(path.read_text()),{'new':'committed'});self.assertTrue(is_private(path))

    def test_real_process_lock_rejects_contention_and_releases_after_kill(self):
        from installer_lock import acquire
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'owner.lock';ready=Path(temporary)/'ready';scripts=Path(__file__).resolve().parent
            code='import sys,time;from pathlib import Path;sys.path.insert(0,sys.argv[1]);from installer_lock import acquire;lock=acquire(Path(sys.argv[2]));Path(sys.argv[3]).write_text("ready");time.sleep(30)'
            child=subprocess.Popen([sys.executable,'-c',code,str(scripts),str(path),str(ready)],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
            try:
                deadline=time.monotonic()+5
                while not ready.exists():
                    if child.poll() is not None:raise AssertionError(child.stderr.read().decode())
                    if time.monotonic()>deadline:raise AssertionError('lock holder not ready')
                    time.sleep(.01)
                with self.assertRaises(BlockingIOError):acquire(path)
                child.kill();child.wait(timeout=5)
                with acquire(path):pass
                self.assertTrue(path.exists())
            finally:
                if child.poll() is None:child.kill();child.wait(timeout=5)
                child.stderr.close()

    def test_install_checkpoints_survive_and_block_concurrent_owners(self):
        from install_state import InstallState
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();calls=[]
            with InstallState(root) as state:
                with self.assertRaisesRegex(ValueError,'Another installer'):InstallState(root).__enter__()
                self.assertTrue(state.phase('fixture','source',lambda:calls.append('installed'),lambda:True))
            with InstallState(root) as state:self.assertFalse(state.phase('fixture','source',lambda:calls.append('unexpected'),lambda:True))
            self.assertEqual(calls,['installed'])

    def test_windows_lock_adapter_seeks_the_same_byte_and_preserves_native_errors(self):
        import errno
        import installer_lock
        from types import SimpleNamespace
        from unittest.mock import patch
        calls=[]
        with tempfile.TemporaryFile() as file:
            def native_lock(descriptor,mode,count):
                calls.append((os.lseek(descriptor,0,os.SEEK_CUR),mode,count))
            windows=SimpleNamespace(name='nt',lseek=os.lseek,SEEK_SET=os.SEEK_SET)
            library=SimpleNamespace(LK_NBLCK=2,locking=native_lock)
            with patch('installer_lock.os',windows),patch.dict(sys.modules,{'msvcrt':library}):
                os.lseek(file.fileno(),8,os.SEEK_SET);installer_lock.exclusive(file.fileno());self.assertEqual(calls,[(0,2,1)])
                for number in (errno.EACCES,errno.EDEADLK,errno.EIO):
                    error=OSError(number,'fixture native error')
                    with patch.object(library,'locking',side_effect=error):
                        with self.assertRaises(OSError) as result:installer_lock.exclusive(file.fileno())
                    if number==errno.EIO:self.assertIs(result.exception,error)
                    else:self.assertIsInstance(result.exception,BlockingIOError)


if __name__=='__main__':unittest.main()
