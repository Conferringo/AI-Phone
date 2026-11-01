"""Disposable CI cleanup contract; every external operation is denied by default."""
import hashlib
from contextlib import ExitStack
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

if os.name != 'posix':
    raise unittest.SkipTest('Linux fixture ownership requires POSIX before importing its owner')

with ExitStack() as import_guards:
    for boundary in ('subprocess.run', 'subprocess.Popen', 'subprocess.check_call',
                     'subprocess.check_output', 'socket.socket', 'urllib.request.build_opener'):
        import_guards.enter_context(patch(boundary, side_effect=AssertionError('Unmocked import operation: ' + boundary)))
    import linux_host_acceptance as acceptance


@unittest.skipUnless(os.name == 'posix', 'Linux CI ownership fixture requires POSIX filesystem identities')
class CleanupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='ai-phone-host-acceptance-')
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name)
        info = self.home.lstat()
        self.identity = (info.st_dev, info.st_ino)
        self.staged = self.home / 'releases' / ('a' * 64)
        self.staged.mkdir(parents=True)
        self.data = self.home / 'user/data'
        self.guest = self.data / 'desktop/home'
        self.guest.mkdir(parents=True)
        (self.staged / 'data').symlink_to(self.data, target_is_directory=True)
        self.calls = []
        names = ('ai-phone-postgres-1', 'ai-phone-redis-1', 'ai-phone-desktop')
        self.ids = {name: hashlib.sha256(name.encode()).hexdigest() for name in names}
        self.effects = {
            (sys.executable, str(self.staged / 'scripts/service_stop.py')): 'service',
            ('sudo', '-n', 'rm', '--one-file-system', '--recursive', '--force', '--', str(self.guest)): 'sudo',
        }
        for identity in self.ids.values():
            self.effects[('docker', 'stop', '--time', '20', identity)] = 'stop'
            self.effects[('docker', 'rm', identity)] = 'remove'
        self.present = set(names)
        self.foreign = None
        self.bind_source = str(self.guest.resolve())
        self.bind_rw = True
        self.fail = None
        self.query_error = False
        self.mutate_after_remove = False
        self.owner_env = {'PATH': str(self.home / 'bin'), 'LIMA_HOME': str(self.home / 'lima')}
        for target in ('subprocess.run', 'subprocess.Popen', 'subprocess.check_call',
                       'subprocess.check_output', 'socket.socket', 'urllib.request.build_opener'):
            guard = patch(target, side_effect=AssertionError('Unmocked external operation: ' + target))
            guard.start()
            self.addCleanup(guard.stop)
        platform = patch.object(sys, 'platform', 'linux')
        platform.start()
        self.addCleanup(platform.stop)
        environment = patch.dict(os.environ, {'GITHUB_ACTIONS': 'true', 'RUNNER_OS': 'Linux'}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)
        runtime = patch('runtime_config.runtime_env', return_value=self.owner_env)
        self.runtime = runtime.start()
        self.addCleanup(runtime.stop)

    def command(self, argv, **options):
        argv = list(map(str, argv))
        self.calls.append((argv, options))
        if self.fail and argv[:len(self.fail)] == self.fail:
            raise subprocess.CalledProcessError(1, argv)
        query = {('docker', 'container', 'ls'): self.query_list,
                 ('docker', 'container', 'inspect'): self.query_inspect}.get(tuple(argv[:3]))
        if query is not None:
            return query(argv)
        if tuple(argv) not in self.effects:
            raise AssertionError('Unexpected mocked command: ' + repr(argv))
        if self.mutate_after_remove and argv == ['docker', 'rm', self.ids['ai-phone-desktop']]:
            self.guest.rename(self.guest.with_name('original-home'))
            self.guest.mkdir()
        return subprocess.CompletedProcess(argv, 0)

    def query_list(self, argv):
        if self.query_error:
            return subprocess.CompletedProcess(argv, 1, stdout='', stderr='untrusted error')
        name = argv[5].removeprefix('name=^/').removesuffix('$')
        return subprocess.CompletedProcess(argv, 0, stdout=self.ids[name] if name in self.present else '', stderr='')

    def query_inspect(self, argv):
        name = next(name for name, identity in self.ids.items() if identity == argv[-1])
        if argv[-2] == '{{json .Mounts}}':
            value = [{'Type': 'bind', 'Source': self.bind_source, 'Destination': '/home/ai', 'RW': self.bind_rw}]
        else:
            owner = '/foreign' if self.foreign == name else str(self.staged)
            value = {'id': self.ids[name], 'running': True,
                     'labels': {'com.docker.compose.project': 'ai-phone',
                                'com.docker.compose.project.working_dir': owner}}
        return subprocess.CompletedProcess(argv, 0, stdout=json.dumps(value), stderr='')

    def cleanup_fixture(self, failure=None):
        with patch('linux_host_acceptance.subprocess.run', side_effect=self.command):
            acceptance.cleanup_fixture(self.home, self.staged, {'PATH': 'wrong-global', 'KEEP': 'fixture'},
                                       self.identity, failure=failure)

    def mutations(self):
        return [args for args, _ in self.calls if args[:2] not in
                (['docker', 'container'],)]

    def test_all_ownership_and_bind_queries_finish_before_any_stop(self):
        self.cleanup_fixture()
        self.runtime.assert_called_once_with(self.staged)
        queries = [args for args, _ in self.calls[:7]]
        self.assertTrue(all(args[:2] == ['docker', 'container'] for args in queries))
        self.assertEqual(queries[-1][-2:], ['{{json .Mounts}}', self.ids['ai-phone-desktop']])
        expected = [[sys.executable, str(self.staged / 'scripts/service_stop.py')]]
        for name in self.ids:
            expected += [['docker', 'stop', '--time', '20', self.ids[name]], ['docker', 'rm', self.ids[name]]]
        expected += [['sudo', '-n', 'rm', '--one-file-system', '--recursive', '--force', '--', str(self.guest)]]
        self.assertEqual(self.mutations(), expected)
        for _, options in self.calls:
            self.assertEqual(options['env'], {'PATH': self.owner_env['PATH'], 'LIMA_HOME': self.owner_env['LIMA_HOME'], 'KEEP': 'fixture'})
            self.assertEqual(options['cwd'], self.staged)
            self.assertLessEqual(options['timeout'], 90)
        self.assertTrue(self.guest.is_dir(), 'The mock must never execute sudo or delete a real directory')

    def test_foreign_third_container_refuses_even_owned_first_two(self):
        self.foreign = 'ai-phone-desktop'
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.mutations(), [])

    def test_verified_staged_data_symlink_source_is_accepted(self):
        self.bind_source = str(self.staged / 'data/desktop/home')
        self.assertNotEqual(self.bind_source, str(self.guest.resolve(strict=True)))
        self.assertEqual(Path(self.bind_source).resolve(strict=True), self.guest.resolve(strict=True))
        self.cleanup_fixture()
        self.assertEqual(self.mutations()[-1],
                         ['sudo', '-n', 'rm', '--one-file-system', '--recursive', '--force', '--', str(self.guest)])

    def test_external_symlink_alias_is_refused_even_if_it_resolves_to_guest(self):
        with tempfile.TemporaryDirectory(prefix='ai-phone-external-alias-') as external:
            alias = Path(external) / 'home-alias'
            alias.symlink_to(self.guest, target_is_directory=True)
            self.bind_source = str(alias)
            self.assertEqual(alias.resolve(strict=True), self.guest.resolve(strict=True))
            with self.assertRaises(ValueError):
                self.cleanup_fixture()
            self.assertEqual(self.mutations(), [])

    def test_unknown_engine_query_is_not_absence(self):
        self.query_error = True
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.mutations(), [])

    def test_malformed_full_identity_is_refused_before_shutdown(self):
        self.ids['ai-phone-desktop'] = 'not-a-full-container-id'
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.mutations(), [])

    def test_wrong_or_readonly_bind_is_not_cleaned(self):
        for source, rw in [('/foreign/home', True), (str(self.guest.resolve()), False)]:
            with self.subTest(source=source, rw=rw):
                self.calls.clear()
                self.bind_source, self.bind_rw = source, rw
                with self.assertRaises(ValueError):
                    self.cleanup_fixture()
                self.assertEqual(self.mutations(), [])

    def test_malformed_ambiguous_or_oversized_mounts_never_authorize_cleanup(self):
        original = self.query_inspect
        home = {'Type': 'bind', 'Source': str(self.guest.resolve()), 'Destination': '/home/ai', 'RW': True}
        for output in ('not-json', 'x' * 16385, '{}', '[null]', '[]', json.dumps([home, home])):
            with self.subTest(output_bytes=len(output)):
                self.calls.clear()
                def inspect(argv):
                    if argv[-2] == '{{json .Mounts}}':
                        return subprocess.CompletedProcess(argv, 0, stdout=output, stderr='')
                    return original(argv)
                with patch.object(self, 'query_inspect', side_effect=inspect):
                    with self.assertRaises(ValueError):
                        self.cleanup_fixture()
                self.assertEqual(self.mutations(), [])

    def test_stop_or_remove_failure_never_reaches_sudo(self):
        for prefix in [['docker', 'stop'], ['docker', 'rm'], [sys.executable]]:
            with self.subTest(prefix=prefix):
                self.calls.clear()
                self.fail = prefix
                with self.assertRaises(subprocess.CalledProcessError):
                    self.cleanup_fixture()
                self.assertFalse(any(args[0] == 'sudo' for args, _ in self.calls))

    def test_both_original_and_cleanup_failure_remain_visible(self):
        original = ValueError('fixture deployment failed')
        self.fail = ['docker', 'stop']
        with self.assertRaises(BaseExceptionGroup) as caught:
            self.cleanup_fixture(failure=original)
        self.assertIs(caught.exception.exceptions[0], original)
        self.assertIsInstance(caught.exception.exceptions[1], subprocess.CalledProcessError)
        self.assertFalse(any(args[0] == 'sudo' for args, _ in self.calls))

    def test_sudo_failure_is_not_swallowed(self):
        self.fail = ['sudo']
        with self.assertRaises(subprocess.CalledProcessError):
            self.cleanup_fixture()

    def test_removed_container_does_not_authorize_replaced_guest_directory(self):
        self.mutate_after_remove = True
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertFalse(any(args[0] == 'sudo' for args, _ in self.calls))

    def test_missing_desktop_never_authorizes_sudo(self):
        self.present.remove('ai-phone-desktop')
        self.cleanup_fixture()
        self.assertFalse(any(args[0] == 'sudo' for args, _ in self.calls))
        removed = [args[-1] for args, _ in self.calls if args[:2] == ['docker', 'rm']]
        self.assertEqual(removed, [self.ids['ai-phone-postgres-1'], self.ids['ai-phone-redis-1']])

    def test_guest_symlink_is_rejected_before_external_queries(self):
        self.guest.rmdir()
        self.guest.symlink_to(self.data, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.calls, [])

    def test_guest_parent_symlink_is_rejected_before_external_queries(self):
        other = self.data / 'other'
        other.mkdir()
        self.guest.rmdir()
        self.guest.parent.rmdir()
        self.guest.parent.symlink_to(other, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.calls, [])

    def test_wrong_persistent_data_link_is_rejected_before_external_queries(self):
        (self.staged / 'data').unlink()
        (self.staged / 'data').symlink_to(self.home, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.calls, [])

    def test_original_temp_identity_and_hash_layout_are_required(self):
        self.identity = (self.identity[0], self.identity[1] + 1)
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.calls, [])

    def test_wrong_staged_path_is_rejected_before_external_queries(self):
        self.staged = self.home / 'user'
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.calls, [])

    def test_no_ci_scope_fails_before_any_external_operation(self):
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(ValueError):
                acceptance.cleanup_fixture(self.home, self.staged, {}, self.identity)
        self.assertEqual(self.calls, [])

    def test_ci_flags_do_not_enable_unmocked_processes(self):
        with self.assertRaisesRegex(AssertionError, 'Unmocked external operation'):
            acceptance.cleanup_fixture(self.home, self.staged, {}, self.identity)
        self.assertEqual(self.calls, [])

    def test_environment_failure_has_no_fallback_to_global_engine(self):
        self.runtime.side_effect = ValueError('invalid selected engine')
        with self.assertRaises(ValueError):
            self.cleanup_fixture()
        self.assertEqual(self.calls, [])


if __name__ == '__main__':
    unittest.main()
