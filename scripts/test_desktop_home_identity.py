"""Owned filesystem fixtures; every account, permission and process action is mocked."""
from contextlib import ExitStack
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

DENIED = ('subprocess.Popen', 'subprocess.run', 'subprocess.call',
          'subprocess.check_call', 'subprocess.check_output', 'os.system',
          'os.chown', 'os.lchown', 'os.fchown', 'os.chmod', 'os.fchmod',
          'os.setuid', 'os.seteuid', 'os.setgid', 'os.setegid', 'os.setgroups',
          'os.setresuid', 'os.setresgid', 'os.kill', 'os.killpg',
          'socket.socket', 'socket.create_connection')

if os.name == 'posix':
    with ExitStack() as guards:
        for target in DENIED:
            guards.enter_context(patch(target, side_effect=AssertionError('Unmocked external operation: ' + target), create=True))
        spec = importlib.util.spec_from_file_location('desktop_home_identity', Path(__file__).resolve().parents[1] / 'infra/desktop/home-identity.py')
        identity = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(identity)


@unittest.skipUnless(os.name == 'posix', 'Container POSIX account semantics')
class DesktopHomeIdentityTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='desktop-home-identity-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.home, self.runtime = self.root / 'home', self.root / 'run'
        self.home.mkdir(mode=0o750)
        self.runtime.mkdir(mode=0o700)
        (self.home / 'private-note').write_bytes(b'fixture user content')
        self.real_lstat = Path.lstat
        self.before = self.snapshot()
        self.home_uid, self.home_gid = 1001, 1001
        self.account = SimpleNamespace(pw_name='ai', pw_uid=1000, pw_gid=1000, pw_dir=str(self.home))
        self.users = {}
        self.groups = {1000: SimpleNamespace(gr_name='ai', gr_gid=1000), 20: SimpleNamespace(gr_name='dialout', gr_gid=20)}
        self.extra_group_names = {}
        self.calls, self.fault = [], ''
        self.parent_mode = 0o700
        for target in DENIED:
            mocked = patch(target, side_effect=AssertionError('Unmocked external operation: ' + target), create=True)
            mocked.start(); self.addCleanup(mocked.stop)
        for target, replacement in [('os.geteuid', lambda: 0), ('pwd.getpwnam', self.getpwnam),
                                    ('pwd.getpwuid', self.getpwuid), ('grp.getgrgid', self.getgrgid),
                                    ('grp.getgrnam', self.getgrnam), ('subprocess.run', self.mock_run)]:
            mocked = patch(target, side_effect=replacement)
            mocked.start(); self.addCleanup(mocked.stop)
        # Bind the fixture method explicitly; Path.lstat's receiver is the path.
        mocked = patch.object(Path, 'lstat', lambda path, *args, **kwargs: self.lstat(path, *args, **kwargs))
        mocked.start(); self.addCleanup(mocked.stop)

    def snapshot(self):
        result = {}
        for path in [self.home, *sorted(self.home.rglob('*'))]:
            metadata = self.real_lstat(path)
            result[str(path.relative_to(self.root))] = (metadata.st_dev, metadata.st_ino, metadata.st_uid,
                metadata.st_gid, metadata.st_mode, path.read_bytes() if path.is_file() else None)
        return result

    def lstat(self, path, *args, **kwargs):
        metadata = self.real_lstat(path, *args, **kwargs)
        uid, gid, mode = metadata.st_uid, metadata.st_gid, metadata.st_mode
        if path == self.home:
            uid, gid = self.home_uid, self.home_gid
        elif path == self.runtime or path.parent == self.runtime:
            uid = 0
            if path == self.runtime:
                mode = (mode & ~0o777) | self.parent_mode
        return SimpleNamespace(st_dev=metadata.st_dev, st_ino=metadata.st_ino, st_uid=uid, st_gid=gid, st_mode=mode)

    def getpwnam(self, name):
        if name != 'ai': raise AssertionError('Unexpected account lookup')
        return self.account

    def getpwuid(self, uid):
        if uid in self.users: return self.users[uid]
        if uid == self.account.pw_uid: return self.account
        raise KeyError(uid)

    def getgrgid(self, gid):
        if gid in self.groups: return self.groups[gid]
        raise KeyError(gid)

    def getgrnam(self, name):
        for group in [*self.groups.values(), *self.extra_group_names.values()]:
            if group.gr_name == name: return group
        raise KeyError(name)

    def mock_run(self, argv, **options):
        argv = list(argv); self.calls.append(argv)
        self.assertTrue(options['check'])
        self.assertEqual(options['stdin'], subprocess.DEVNULL)
        self.assertEqual(options['stdout'], subprocess.DEVNULL)
        self.assertEqual(options['stderr'], subprocess.PIPE)
        if argv[0] == 'groupadd':
            self.assertEqual(argv, ['groupadd', '--gid', str(self.home_gid), 'ai-home-' + str(self.home_gid)])
            if self.fault == 'group': raise RuntimeError('group creation failure')
            self.groups[self.home_gid] = SimpleNamespace(gr_name=argv[-1], gr_gid=self.home_gid)
        elif argv[0] == 'usermod' and '--uid' in argv:
            self.assertEqual(options['timeout'], 15)
            absent = Path(argv[2])
            self.assertEqual(absent.name, 'absent-home')
            self.assertEqual(absent.parent.parent, self.runtime)
            self.assertFalse(absent.exists())
            self.assertNotIn('-m', argv); self.assertNotIn('--move-home', argv)
            self.assertNotIn('-o', argv); self.assertNotIn('--non-unique', argv)
            uid, gid = (1000, 1000) if self.home_uid == 0 else (self.home_uid, self.home_gid)
            self.assertEqual(argv[3:], ['--uid', str(uid), '--gid', str(gid), 'ai'])
            if self.fault == 'first': raise RuntimeError('account change failure')
            self.account = SimpleNamespace(pw_name='ai', pw_uid=uid, pw_gid=gid, pw_dir=str(absent))
            if self.fault == 'both': raise RuntimeError('account change failure')
            if self.fault == 'unexpected-child': absent.mkdir()
            if self.fault == 'replace-temporary':
                folder = absent.parent
                folder.rename(folder.with_name(folder.name + '-preserved'))
                folder.mkdir(mode=0o700)
        elif argv[0] == 'usermod':
            self.assertEqual(argv, ['usermod', '--home', str(self.home), 'ai'])
            if self.fault in ('both', 'restore'): raise RuntimeError('home restoration failure')
            if self.fault != 'ignored-restore': self.account.pw_dir = str(self.home)
        elif argv[0] == 'find':
            self.assertEqual(self.home_uid, 0, 'Nonroot home must never be recursively chowned')
            self.assertEqual(argv, ['find', str(self.home), '(', '-type', 'd', '-o', '-type', 'f', ')', '-exec', 'chown', '1000:1000', '{}', '+'])
            self.assertIsNone(options['timeout'])
        else:
            raise AssertionError('Unexpected mocked command')
        return subprocess.CompletedProcess(argv, 0)

    def initialize(self):
        return identity.initialize(self.home, self.runtime)

    def assert_preserved(self):
        self.assertEqual(self.snapshot(), self.before)

    def error_text(self, error):
        return '\n'.join(self.error_text(child) for child in error.exceptions) if isinstance(error, BaseExceptionGroup) else str(error)

    def test_nonroot_home_aligns_account_without_ownership_changes(self):
        self.assertEqual(self.initialize(), (1001, 1001))
        self.assertEqual([args[0] for args in self.calls], ['groupadd', 'usermod', 'usermod'])
        self.assertEqual((self.account.pw_uid, self.account.pw_gid, self.account.pw_dir), (1001, 1001, str(self.home)))
        self.assertEqual(list(self.runtime.iterdir()), [])
        self.assert_preserved()

    def test_existing_gid_is_reused_without_renumbering_ai_or_other_groups(self):
        self.home_gid = 20
        self.assertEqual(self.initialize(), (1001, 20))
        self.assertEqual([args[0] for args in self.calls], ['usermod', 'usermod'])
        self.assertEqual(self.groups[20].gr_name, 'dialout')
        self.assertEqual(self.groups[1000].gr_gid, 1000)
        self.assert_preserved()

    def test_existing_1000_home_needs_no_account_or_permission_command(self):
        self.home_uid = self.home_gid = 1000
        self.assertEqual(self.initialize(), (1000, 1000))
        self.assertEqual(self.calls, [])
        self.assert_preserved()

    def test_root_owned_home_retains_1000_compatibility_repair_only(self):
        self.home_uid = self.home_gid = 0
        self.assertEqual(self.initialize(), (1000, 1000))
        self.assertEqual([args[0] for args in self.calls], ['find'])
        self.assert_preserved()

    def test_root_owned_home_keeps_1000_even_after_an_earlier_account_mapping(self):
        self.home_uid = self.home_gid = 0
        self.account.pw_uid = self.account.pw_gid = 1001
        self.assertEqual(self.initialize(), (1000, 1000))
        self.assertEqual([args[0] for args in self.calls], ['usermod', 'usermod', 'find'])
        self.assert_preserved()

    def test_repeated_initialization_is_stable(self):
        self.initialize(); self.calls.clear()
        self.assertEqual(self.initialize(), (1001, 1001))
        self.assertEqual(self.calls, [])
        self.assert_preserved()

    def test_conflicting_uid_fails_before_group_or_account_changes(self):
        self.users[1001] = SimpleNamespace(pw_name='another', pw_uid=1001)
        with self.assertRaisesRegex(ValueError, 'another container account'): self.initialize()
        self.assertEqual(self.calls, []); self.assert_preserved()

    def test_conflicting_owned_group_name_is_not_adopted(self):
        self.extra_group_names['occupied'] = SimpleNamespace(gr_name='ai-home-1001', gr_gid=2000)
        with self.assertRaisesRegex(ValueError, 'group name is occupied'): self.initialize()
        self.assertEqual(self.calls, []); self.assert_preserved()

    def test_group_failure_does_not_change_the_account_or_home(self):
        self.fault = 'group'
        with self.assertRaisesRegex(RuntimeError, 'group creation failure'): self.initialize()
        self.assertEqual(self.account.pw_dir, str(self.home))
        self.assertEqual(list(self.runtime.iterdir()), []); self.assert_preserved()

    def test_account_failure_still_restores_fixed_home_and_refuses_success(self):
        self.fault = 'first'
        with self.assertRaises(BaseExceptionGroup) as caught: self.initialize()
        self.assertIn('account change failure', self.error_text(caught.exception))
        self.assertEqual(self.calls[-1], ['usermod', '--home', str(self.home), 'ai'])
        self.assertEqual(self.account.pw_dir, str(self.home))
        self.assertEqual(list(self.runtime.iterdir()), []); self.assert_preserved()

    def test_primary_and_restore_failures_are_both_retained(self):
        self.fault = 'both'
        with self.assertRaises(BaseExceptionGroup) as caught: self.initialize()
        text = self.error_text(caught.exception)
        self.assertIn('account change failure', text); self.assertIn('home restoration failure', text)
        self.assertEqual(list(self.runtime.iterdir()), []); self.assert_preserved()

    def test_restore_failure_after_successful_account_change_refuses_success(self):
        self.fault = 'restore'
        with self.assertRaises(BaseExceptionGroup) as caught: self.initialize()
        self.assertIn('home restoration failure', self.error_text(caught.exception))
        self.assert_preserved()

    def test_successful_exit_without_fixed_home_restoration_is_rejected(self):
        self.fault = 'ignored-restore'
        with self.assertRaises(BaseExceptionGroup) as caught: self.initialize()
        self.assertIn('home restoration was not verified', self.error_text(caught.exception))
        self.assert_preserved()

    def test_unexpected_temporary_contents_are_preserved_as_cleanup_failure(self):
        self.fault = 'unexpected-child'
        with self.assertRaises(BaseExceptionGroup): self.initialize()
        folders = list(self.runtime.iterdir())
        self.assertEqual(len(folders), 1)
        self.assertTrue((folders[0] / 'absent-home').is_dir())
        self.assertEqual(self.account.pw_dir, str(self.home)); self.assert_preserved()

    def test_replaced_temporary_identity_is_not_removed(self):
        self.fault = 'replace-temporary'
        with self.assertRaises(BaseExceptionGroup) as caught: self.initialize()
        self.assertIn('temporary directory changed', self.error_text(caught.exception))
        self.assertEqual(len(list(self.runtime.iterdir())), 2)
        self.assert_preserved()

    def test_uncontrolled_runtime_parent_refuses_account_changes(self):
        self.home_gid = 20; self.parent_mode = 0o777
        with self.assertRaisesRegex(ValueError, 'privately controlled'): self.initialize()
        self.assertEqual(self.calls, []); self.assert_preserved()

    def test_nonroot_caller_is_rejected_before_account_changes(self):
        with patch('os.geteuid', return_value=1001):
            with self.assertRaisesRegex(ValueError, 'container root'): self.initialize()
        self.assertEqual(self.calls, []); self.assert_preserved()

    def test_non_fixed_account_home_is_not_implicitly_adopted(self):
        self.account.pw_dir = '/unexpected-home'
        with self.assertRaisesRegex(ValueError, 'fixed directory'): self.initialize()
        self.assertEqual(self.calls, []); self.assert_preserved()


if __name__ == '__main__':
    unittest.main()
