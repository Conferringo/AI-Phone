"""Pure mocked CI identity checks; no host permission, process, network or device access."""
import os
import unittest

if os.name != 'posix':
    raise unittest.SkipTest('CI primary-group identity fixtures require POSIX before Linux owner import')

from contextlib import ExitStack
import grp
import pwd
import socket
import subprocess
from types import SimpleNamespace
from unittest.mock import patch
import urllib.request


def deny_external(stack):
    for name in ('run', 'Popen', 'call', 'check_call', 'check_output', 'getoutput', 'getstatusoutput'):
        stack.enter_context(patch.object(subprocess, name, side_effect=AssertionError('External process forbidden')))
    for name in ('system', 'fork', 'posix_spawn', 'posix_spawnp', 'execv', 'execve', 'execvp', 'execvpe',
                 'setuid', 'seteuid', 'setgid', 'setegid', 'setgroups'):
        if hasattr(os, name):
            stack.enter_context(patch.object(os, name, side_effect=AssertionError('Host execution or permission change forbidden')))
    for name in ('urlopen', 'build_opener'):
        stack.enter_context(patch.object(urllib.request, name, side_effect=AssertionError('Network request forbidden')))
    stack.enter_context(patch.object(socket, 'create_connection', side_effect=AssertionError('Network connection forbidden')))
    for name in ('connect', 'connect_ex', 'bind'):
        stack.enter_context(patch.object(socket.socket, name, side_effect=AssertionError('Network socket operation forbidden')))


# Importing the Linux owner does not authorize external execution, even on a POSIX host.
with ExitStack() as import_guard:
    deny_external(import_guard)
    import linux_host_acceptance as acceptance


class PrimaryGroupTests(unittest.TestCase):
    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        deny_external(self.stack)
        self.identity = {}
        for name, value in (('getuid', 1001), ('geteuid', 1001), ('getgid', 1001),
                            ('getegid', 1001), ('getgroups', [27, 993, 998])):
            self.identity[name] = self.stack.enter_context(patch.object(os, name, return_value=value))
        self.account = self.stack.enter_context(patch.object(pwd, 'getpwuid', return_value=SimpleNamespace(pw_uid=1001, pw_gid=1001)))
        self.kvm = self.stack.enter_context(patch.object(grp, 'getgrnam', return_value=SimpleNamespace(gr_gid=993)))

    def test_original_nonroot_identity_with_kvm_supplementary_group_is_accepted(self):
        self.assertIsNone(acceptance.require_ci_primary_group())
        self.account.assert_called_once_with(1001)
        self.kvm.assert_called_once_with('kvm')

    def test_kvm_as_passwd_primary_group_is_also_effective_membership(self):
        self.account.return_value.pw_gid = 993
        self.identity['getgid'].return_value = 993
        self.identity['getegid'].return_value = 993
        self.identity['getgroups'].return_value = [27]
        self.assertIsNone(acceptance.require_ci_primary_group())

    def test_root_or_changed_effective_uid_is_rejected(self):
        for uid, euid in ((0, 0), (1001, 0), (1001, 1002)):
            with self.subTest(uid=uid, euid=euid):
                self.identity['getuid'].return_value = uid
                self.identity['geteuid'].return_value = euid
                with self.assertRaisesRegex(ValueError, 'original non-root UID'):
                    acceptance.require_ci_primary_group()
        self.account.assert_not_called()
        self.kvm.assert_not_called()

    def test_real_and_effective_gid_must_both_match_passwd_primary_group(self):
        for gid, egid in ((993, 993), (993, 1001), (1001, 993)):
            with self.subTest(gid=gid, egid=egid):
                self.identity['getgid'].return_value = gid
                self.identity['getegid'].return_value = egid
                with self.assertRaisesRegex(ValueError, 'passwd identity and primary group'):
                    acceptance.require_ci_primary_group()

    def test_foreign_passwd_uid_is_rejected(self):
        self.account.return_value.pw_uid = 1002
        with self.assertRaisesRegex(ValueError, 'passwd identity and primary group'):
            acceptance.require_ci_primary_group()

    def test_unrelated_groups_do_not_replace_kvm_membership(self):
        self.identity['getgroups'].return_value = [27, 998, 1001]
        with self.assertRaisesRegex(ValueError, 'requires KVM group access'):
            acceptance.require_ci_primary_group()

    def test_missing_account_or_kvm_group_is_an_explicit_failure(self):
        for lookup in (self.account, self.kvm):
            with self.subTest(lookup=lookup):
                lookup.side_effect = KeyError('fixture lookup missing')
                with self.assertRaisesRegex(ValueError, 'could not be verified') as error:
                    acceptance.require_ci_primary_group()
                self.assertIsInstance(error.exception.__cause__, KeyError)
                lookup.side_effect = None

    def test_group_query_failure_does_not_authorize_deployment(self):
        self.identity['getgroups'].side_effect = OSError('fixture group query unavailable')
        with self.assertRaisesRegex(ValueError, 'could not be verified') as error:
            acceptance.require_ci_primary_group()
        self.assertIsInstance(error.exception.__cause__, OSError)

    def test_main_rejects_wrong_primary_group_before_preparing_any_fixture(self):
        self.identity['getgid'].return_value = 993
        self.identity['getegid'].return_value = 993
        with patch.dict(os.environ, {'GITHUB_ACTIONS': 'true', 'RUNNER_OS': 'Linux'}), \
             patch.object(acceptance.tempfile, 'TemporaryDirectory', side_effect=AssertionError('Fixture creation forbidden')) as temporary, \
             patch.object(acceptance, 'prepare', side_effect=AssertionError('Preparation forbidden')) as prepare, \
             patch.object(acceptance, 'stage_source', side_effect=AssertionError('Staging forbidden')) as stage:
            with self.assertRaisesRegex(ValueError, 'passwd identity and primary group'):
                acceptance.main()
        temporary.assert_not_called()
        prepare.assert_not_called()
        stage.assert_not_called()

    def test_main_outside_ci_rejects_before_identity_or_fixture_work(self):
        with patch.dict(os.environ, {'GITHUB_ACTIONS': 'false', 'RUNNER_OS': 'Linux'}), \
             patch.object(acceptance, 'require_ci_primary_group') as identity, \
             patch.object(acceptance.tempfile, 'TemporaryDirectory', side_effect=AssertionError('Fixture creation forbidden')) as temporary:
            with self.assertRaisesRegex(ValueError, 'restricted to an ephemeral Linux CI runner'):
                acceptance.main()
        identity.assert_not_called()
        temporary.assert_not_called()


if __name__ == '__main__':
    unittest.main()
