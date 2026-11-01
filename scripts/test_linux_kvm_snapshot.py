"""Fixed-device diagnostic contracts; no host device is accessed."""
import os
import unittest

if os.name != 'posix':
    raise unittest.SkipTest('POSIX fixed-device diagnostic fixtures')

import contextlib
import builtins
import errno
import io
import json
import stat
import struct
import subprocess
import tempfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import android_boot_diagnostics as diagnostics
import linux_kvm_snapshot as kvm


def character(**values):
    return SimpleNamespace(**({'st_mode': stat.S_IFCHR | 0o660, 'st_uid': 0, 'st_gid': 108,
                              'st_dev': 1, 'st_ino': 42, 'st_rdev': 2792} | values))


@contextlib.contextmanager
def device_fixture():
    info = character()
    with contextlib.ExitStack() as stack:
        def mocked(target, **values):
            return stack.enter_context(patch(target, **values))
        mocked('linux_kvm_snapshot.platform.system', return_value='Linux')
        mocked('linux_kvm_snapshot.os.getuid', return_value=1001)
        mocked('linux_kvm_snapshot.os.geteuid', return_value=1001)
        mocked('linux_kvm_snapshot.os.getegid', return_value=1001)
        mocked('linux_kvm_snapshot.os.getgroups', return_value=[])
        handles = {
            'acl': mocked('linux_kvm_snapshot.os.getxattr', side_effect=OSError(errno.ENODATA, 'absent')),
            'groups': kvm.os.getgroups,
            'egid': kvm.os.getegid,
            'stat': mocked('linux_kvm_snapshot.os.lstat', return_value=info),
            'access': mocked('linux_kvm_snapshot.os.access', return_value=True),
            'open': mocked('linux_kvm_snapshot.os.open', return_value=73),
            'fstat': mocked('linux_kvm_snapshot.os.fstat', return_value=info),
            'close': mocked('linux_kvm_snapshot.os.close'),
            'ioctl': mocked('fcntl.ioctl', return_value=12),
        }
        yield handles


class GuardedTests(unittest.TestCase):
    def setUp(self):
        stack = contextlib.ExitStack()
        self.addCleanup(stack.close)
        fields = {name: getattr(os, name) for name in ('R_OK', 'W_OK', 'O_RDWR', 'O_NONBLOCK', 'O_NOFOLLOW', 'O_CLOEXEC')}
        fields['environ'] = os.environ
        for name in ('getuid', 'geteuid', 'getegid', 'getgroups', 'lstat', 'access', 'open', 'fstat', 'close', 'getxattr'):
            fields[name] = Mock(side_effect=AssertionError('Unmocked hardware read: ' + name))
        stack.enter_context(patch.object(kvm, 'os', SimpleNamespace(**fields)))
        stack.enter_context(patch('fcntl.ioctl', side_effect=AssertionError('Unmocked ioctl')))
        for name in ('Popen', 'run', 'call', 'check_call', 'check_output'):
            stack.enter_context(patch.object(subprocess, name, side_effect=AssertionError('No external process')))


class KvmObservationTests(GuardedTests):
    def test_boot_failure_retains_kvm_permission_evidence(self):
        with patch.object(diagnostics, 'resources', return_value={}), \
             patch.object(diagnostics, 'process_report', return_value={}), \
             patch.object(diagnostics, 'applied_configuration', return_value={}), \
             patch.object(diagnostics, 'acceleration', return_value={'return_code': 11}), \
             patch.object(diagnostics, 'boot_probes', return_value={}), \
             patch.object(kvm, 'snapshot', return_value={'platform': 'linux', 'ioctl': {'error': 'permission_denied'}}):
            value = diagnostics.collect(Path('/unused'), ['unused'], 'owned_child_exited',
                                        {'observations': 2, 'last_boot': {}}, {}, 0, clock=lambda: 1)
        self.assertIn('kvm', value, 'code 11 currently loses the fixed device permission snapshot')
        self.assertEqual(value['kvm']['ioctl']['error'], 'permission_denied')
        self.assertEqual(value['phase'], 'owned_child_exited')
        self.assertEqual(value['acceleration'], {'return_code': 11})
        self.assertFalse(value['oom_cause_confirmed'])
        self.assertLess(len(json.dumps(value).encode()), 16384)

    def test_module_is_loaded_from_this_candidate(self):
        scripts = Path(__file__).resolve().parent
        self.assertEqual(Path(kvm.__file__).resolve().parent, scripts)
        self.assertEqual(Path(diagnostics.__file__).resolve().parent, scripts)

    def test_success_is_fixed_node_read_only_ioctl_and_fixed_numeric_fields(self):
        with device_fixture() as mock:
            value = kvm.snapshot()
        self.assertEqual(set(value), {'platform', 'override_present', 'identity', 'device', 'access', 'ioctl', 'acl', 'node_group_member'})
        self.assertEqual(value['identity'], {'uid': 1001, 'euid': 1001})
        self.assertEqual(value['device'], {'kind': 'character', 'mode': 0o660, 'uid': 0, 'gid': 108,
                                         'dev': 1, 'inode': 42, 'rdev': 2792, 'error': None})
        self.assertEqual(value['ioctl'], {'status': 'ready', 'version': 12, 'error': None})
        self.assertEqual(mock['open'].call_args.args,
                         ('/dev/kvm', os.O_RDWR | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC))
        mock['ioctl'].assert_called_once_with(73, 0xAE00, 0)
        mock['close'].assert_called_once_with(73)
        self.assertTrue(all(call.args[0] == '/dev/kvm' for call in mock['stat'].call_args_list))
        self.assertEqual([call.kwargs['effective_ids'] for call in mock['access'].call_args_list],
                         [False, False, True, True])
        self.assertTrue(all(call.kwargs['follow_symlinks'] is False for call in mock['access'].call_args_list))

    def test_open_permission_failure_is_observation_and_never_leaks_error_text(self):
        with device_fixture() as mock:
            mock['access'].return_value = False
            mock['open'].side_effect = PermissionError(errno.EACCES, 'private user and secret path')
            value = kvm.snapshot()
        self.assertEqual(value['ioctl'], {'status': 'unknown', 'version': None, 'error': 'permission_denied'})
        self.assertFalse(value['access']['real']['read'])
        mock['ioctl'].assert_not_called()
        mock['close'].assert_not_called()
        self.assertNotIn('private', json.dumps(value))

    def test_missing_permission_and_unrecognized_stat_errors_keep_unknowns(self):
        for code, expected in ((errno.ENOENT, 'missing'), (errno.EPERM, 'permission_denied'), (errno.EIO, 'unknown')):
            with self.subTest(code=code), device_fixture() as mock:
                mock['stat'].side_effect = OSError(code, 'private diagnostic detail')
                value = kvm.snapshot()
                self.assertEqual(value['device']['error'], expected)
                self.assertIsNone(value['access']['real']['read'])
                self.assertEqual(value['ioctl']['status'], 'not_probed')
                mock['open'].assert_not_called()
                mock['access'].assert_not_called()

    def test_effective_access_unavailable_is_unknown_not_denied(self):
        with device_fixture() as mock:
            def check(_path, _mode, **options):
                if options['effective_ids']:
                    raise NotImplementedError('unsupported on fixture')
                return True
            mock['access'].side_effect = check
            value = kvm.snapshot()
        self.assertEqual(value['access'], {'real': {'read': True, 'write': True},
                                          'effective': {'read': None, 'write': None}})

    def test_temporary_regular_file_and_symlink_are_never_opened_as_device(self):
        with tempfile.TemporaryDirectory() as temporary:
            file = Path(temporary)/'owned-file'
            file.write_text('preserve bytes')
            link = Path(temporary)/'owned-link'
            link.symlink_to(file)
            records = ((file.lstat(), 'other'), (link.lstat(), 'symlink'))
            for info, kind in records:
                with self.subTest(kind=kind), device_fixture() as mock:
                    mock['stat'].return_value = info
                    value = kvm.snapshot()
                    self.assertEqual(value['device']['kind'], kind)
                    self.assertEqual(value['ioctl']['status'], 'not_probed')
                    mock['open'].assert_not_called()
                    mock['access'].assert_not_called()
            self.assertEqual(file.read_text(), 'preserve bytes')
            self.assertTrue(link.is_symlink())

    def test_replaced_open_device_is_closed_without_ioctl(self):
        with device_fixture() as mock:
            mock['fstat'].return_value = character(st_ino=99)
            value = kvm.snapshot()
        self.assertEqual(value['ioctl']['error'], 'changed')
        mock['ioctl'].assert_not_called()
        mock['close'].assert_called_once_with(73)

    def test_node_replaced_after_ioctl_does_not_claim_stable_identity(self):
        for replacement in (character(st_ino=99), character(st_mode=stat.S_IFCHR | 0o600), character(st_uid=42)):
            with self.subTest(replacement=replacement), device_fixture() as mock:
                mock['stat'].side_effect = [character(), replacement]
                value = kvm.snapshot()
            self.assertEqual(value['device']['error'], 'changed')
            self.assertEqual(value['ioctl']['version'], 12)

    def test_ioctl_errors_and_unexpected_version_are_not_success(self):
        for code, expected in ((errno.EACCES, 'permission_denied'), (errno.ENOTTY, 'unsupported'), (errno.EIO, 'unknown')):
            with self.subTest(code=code), device_fixture() as mock:
                mock['ioctl'].side_effect = OSError(code, 'private kernel error')
                value = kvm.snapshot()
                self.assertEqual(value['ioctl']['error'], expected)
                self.assertEqual(value['ioctl']['status'], 'unknown')
                mock['close'].assert_called_once_with(73)
        with device_fixture() as mock:
            mock['ioctl'].return_value = 11
            self.assertEqual(kvm.snapshot()['ioctl'], {'status': 'api_mismatch', 'version': 11, 'error': None})

    def test_override_is_presence_only_and_never_changes_the_probed_node(self):
        with patch.dict(os.environ, {'ANDROID_EMULATOR_KVM_DEVICE': '/private/secret/override'}), device_fixture() as mock:
            value = kvm.snapshot()
        self.assertTrue(value['override_present'])
        self.assertEqual(mock['open'].call_args.args[0], '/dev/kvm')
        self.assertNotIn('/private', json.dumps(value))

    def test_non_linux_never_reads_node_or_process_identity(self):
        with device_fixture() as mock, patch.object(kvm.platform, 'system', return_value='Darwin'), \
             patch.object(kvm.os, 'getuid', side_effect=AssertionError('No Linux probe')):
            self.assertEqual(kvm.snapshot(), {'platform': 'unsupported'})
            mock['stat'].assert_not_called()
            mock['open'].assert_not_called()

    def test_optional_probe_and_output_failure_never_changes_flow(self):
        with patch.object(kvm.platform, 'system', side_effect=OSError('private')):
            self.assertEqual(kvm.snapshot(), {'platform': 'unknown'})
        with patch.object(kvm, 'snapshot', side_effect=OSError('private')):
            kvm.emit_snapshot('preflight_ready')
        with patch.object(kvm, 'snapshot', return_value={'platform': 'unknown'}), \
             patch('builtins.print', side_effect=BrokenPipeError('private')):
            kvm.emit_snapshot('engine_ready')

    def test_checkpoints_are_fixed_fields_and_reject_unrecognized_phase(self):
        output = io.StringIO()
        with device_fixture(), contextlib.redirect_stdout(output):
            kvm.emit_snapshot('before_android_start')
        line = output.getvalue()
        self.assertTrue(line.startswith('AI_PHONE_KVM '))
        self.assertLess(len(line.encode()), 4096)
        value = json.loads(line.split(' ', 1)[1])
        self.assertEqual(set(value), {'format', 'phase', 'kvm'})
        self.assertEqual(value['phase'], 'before_android_start')
        with patch.object(kvm, 'snapshot') as probe, self.assertRaisesRegex(ValueError, 'Invalid KVM'):
            kvm.emit_snapshot('/private/phase')
        probe.assert_not_called()


def acl(*entries, version=2):
    return struct.pack('<I', version) + b''.join(struct.pack('<HHI', *entry) for entry in entries)


def access_acl(user=7, mask=6):
    return acl((1, 6, 0xffffffff), (2, user, 1001), (2, 7, 405555),
               (4, 6, 0xffffffff), (8, 7, 406666), (16, mask, 0xffffffff), (32, 0, 0xffffffff))


class AclProjectionTests(GuardedTests):
    def test_non_posix_discovery_skips_before_importing_any_hardware_owner(self):
        original_import = builtins.__import__
        def guarded_import(name, *args, **kwargs):
            if name == 'os':
                return SimpleNamespace(name='nt')
            if name in ('fcntl', 'android_boot_diagnostics', 'linux_kvm_snapshot', 'linux_host'):
                raise AssertionError('Hardware owner imported before skip')
            return original_import(name, *args, **kwargs)
        scope = {'__name__': 'non_posix_fixture', '__file__': __file__,
                 '__builtins__': {**vars(builtins), '__import__': guarded_import}}
        with self.assertRaises(unittest.SkipTest) as caught:
            exec(compile(Path(__file__).read_text(), __file__, 'exec'), scope)
        self.assertEqual(str(caught.exception), 'POSIX fixed-device diagnostic fixtures')

    def observe(self, body, **identity):
        with device_fixture() as mock:
            mock['acl'].side_effect = None
            mock['acl'].return_value = body
            if identity:
                mock['stat'].return_value = character(**identity)
                mock['fstat'].return_value = character(**identity)
            return kvm.snapshot(), mock

    def test_current_effective_uid_and_mask_only_with_fixed_nofollow_read(self):
        value, mock = self.observe(access_acl(mask=0))
        self.assertEqual(value['acl'], {'status': 'present', 'euid_permissions': 7, 'mask_permissions': 0, 'error': None})
        mock['acl'].assert_called_once_with('/dev/kvm', 'system.posix_acl_access', follow_symlinks=False)
        self.assertFalse(value['node_group_member'])
        self.assertEqual(value['ioctl']['status'], 'ready')
        self.assertNotIn('405555', json.dumps(value))
        self.assertNotIn('406666', json.dumps(value))
        self.assertLess(len(json.dumps(value).encode()), 4096)

    def test_acl_absence_permission_failure_and_unsupported_are_distinct(self):
        for code, status, error in [(errno.ENODATA, 'absent', None), (errno.EACCES, 'unknown', 'permission_denied'), (errno.ENOTSUP, 'unknown', 'unsupported')]:
            with self.subTest(code=code), device_fixture() as mock:
                mock['acl'].side_effect = OSError(code, 'private full ACL detail')
                value = kvm.snapshot()
                self.assertEqual(value['acl'], {'status': status, 'euid_permissions': None, 'mask_permissions': None, 'error': error})
                self.assertNotIn('private', json.dumps(value))
                self.assertEqual(value['ioctl']['status'], 'ready')

    def test_uid_projection_uses_effective_identity_and_omits_unmatched_entry(self):
        with device_fixture() as mock, patch.object(kvm.os, 'geteuid', return_value=2002):
            mock['acl'].side_effect = None
            mock['acl'].return_value = access_acl()
            value = kvm.snapshot()
        self.assertEqual(value['acl']['status'], 'present')
        self.assertIsNone(value['acl']['euid_permissions'])
        self.assertEqual(value['acl']['mask_permissions'], 6)

    def test_group_membership_is_numeric_boolean_or_unknown_without_group_inventory(self):
        for effective, groups, expected in [(108, [], True), (1001, [108, 777777], True), (1001, [777777], False)]:
            with self.subTest(expected=expected), device_fixture() as mock:
                mock['egid'].return_value = effective
                mock['groups'].return_value = groups
                value = kvm.snapshot()
            self.assertIs(value['node_group_member'], expected)
            self.assertNotIn('777777', json.dumps(value))
        with device_fixture() as mock:
            mock['groups'].side_effect = OSError('private groups')
            self.assertIsNone(kvm.snapshot()['node_group_member'])
        with device_fixture() as mock:
            mock['groups'].return_value = [108] * 65537
            self.assertIsNone(kvm.snapshot()['node_group_member'])

    def test_malformed_or_over_budget_acl_never_projects_partial_permissions(self):
        bodies = [b'', b'\x02\x00\x00', access_acl() + b'x', access_acl()[::-1], access_acl(user=8), access_acl(mask=8),
                  acl((1, 6, 0xffffffff), (2, 7, 1001), (4, 6, 0xffffffff), (32, 0, 0xffffffff)),
                  access_acl() + struct.pack('<HHI', 16, 7, 0xffffffff),
                  access_acl() + struct.pack('<HHI', 64, 7, 0xffffffff),
                  access_acl() + struct.pack('<HHI', 2, 7, 1001),
                  acl((1, 6, 0), (4, 6, 0xffffffff), (32, 0, 0xffffffff)), b'x' * 4100]
        for body in bodies:
            with self.subTest(size=len(body)):
                value, _ = self.observe(body)
                self.assertEqual(value['acl']['status'], 'unknown')
                self.assertIsNone(value['acl']['euid_permissions'])
                self.assertIsNone(value['acl']['mask_permissions'])

    def test_acl_permission_and_byte_boundaries_preserve_zero_and_absent_entries(self):
        for permissions in range(8):
            value, _ = self.observe(access_acl(user=permissions, mask=permissions))
            self.assertEqual(value['acl']['euid_permissions'], permissions)
            self.assertEqual(value['acl']['mask_permissions'], permissions)
        value, _ = self.observe(acl((1, 6, 0xffffffff), (4, 6, 0xffffffff), (32, 0, 0xffffffff)))
        self.assertEqual(value['acl'], {'status': 'present', 'euid_permissions': None, 'mask_permissions': None, 'error': None})
        entries = [(1, 6, 0xffffffff), *[(2, 7, 2000 + i) for i in range(507)],
                   (4, 6, 0xffffffff), (16, 6, 0xffffffff), (32, 0, 0xffffffff)]
        body = acl(*entries)
        self.assertEqual(len(body), 4092)
        self.assertEqual(self.observe(body)[0]['acl']['status'], 'present')
        self.assertEqual(self.observe(body + struct.pack('<HHI', 2, 7, 9999))[0]['acl']['error'], 'size_limit')

    def test_node_identity_change_or_unreadable_afterwards_discards_acl_projection(self):
        for after in (character(st_ino=99), character(st_mode=stat.S_IFCHR | 0o600), PermissionError(errno.EACCES, 'private')):
            with self.subTest(after=type(after).__name__), device_fixture() as mock:
                mock['acl'].side_effect = None
                mock['acl'].return_value = access_acl()
                mock['stat'].side_effect = [character(), after]
                value = kvm.snapshot()
            self.assertEqual(value['acl']['status'], 'unknown')
            self.assertIsNone(value['acl']['euid_permissions'])
            self.assertIsNone(value['acl']['mask_permissions'])
            self.assertIsNone(value['node_group_member'])

    def test_unopened_node_and_non_linux_never_read_acl_or_groups(self):
        for mode in (stat.S_IFLNK | 0o777, stat.S_IFREG | 0o600):
            value, mock = self.observe(access_acl(), st_mode=mode)
            mock['acl'].assert_not_called()
            mock['groups'].assert_not_called()
        with device_fixture() as mock, patch.object(kvm.platform, 'system', return_value='Darwin'):
            self.assertEqual(kvm.snapshot(), {'platform': 'unsupported'})
            mock['acl'].assert_not_called()
            mock['groups'].assert_not_called()


class CheckpointConsumerTests(GuardedTests):
    def installation_steps(self, failure=None, bundled=False):
        import client_host
        calls = []
        problem = RuntimeError('fixture stops at ' + str(failure))
        def step(name):
            calls.append(name)
            if name == failure:
                raise problem
        with contextlib.ExitStack() as stack:
            def mocked(target, **values):
                return stack.enter_context(patch(target, **values))
            mocked('client_host.check_request', return_value={})
            mocked('host_platform.host_policy', return_value=({'tools': [], 'sdk_hosts': {'fixture': {}}, 'minimum_resume_free_gb': 4}, {'sdk': 'fixture'}))
            mocked('prebuilt_services.launch_spec', return_value={'prebuilt': True})
            mocked('client_host.install_preflight', return_value=True)
            mocked('client_host.deployment_sdk', return_value=Path('/fixture/sdk') if bundled else None)
            mocked('android_runtime.bundled_sdk', return_value={} if bundled else None)
            mocked('prebuilt_phone.available', return_value=True)
            mocked('client_host.install_tools', side_effect=lambda *_: step('tools'))
            mocked('client_host.prepare_profile', side_effect=lambda *_: step('profile'))
            mocked('runtime_config.runtime_env', return_value={'AI_PHONE_SDK_ROOT': '/fixture/sdk'})
            mocked('client_host.prepare_sdk', side_effect=lambda *_: step('sdk'))
            mocked('client_host.container_engine', side_effect=lambda *_: step('engine'))
            mocked('client_host.verify_container_ownership', side_effect=lambda *_: step('ownership'))
            mocked('release.install', side_effect=lambda **_: step('release'))
            mocked('prebuilt_js.launch_spec', return_value={'argv': ['fixture-device-setup']})
            mocked('client_host.execute', side_effect=lambda *_args, **_kwargs: step('device-setup'))
            mocked('linux_kvm_snapshot.emit_snapshot', side_effect=lambda phase: calls.append(phase))
            stack.enter_context(patch.dict(os.environ, {}, clear=True))
            if failure:
                with self.assertRaises(RuntimeError) as caught:
                    client_host.install(Path('/fixture/root'), Path('/fixture/home'), {}, None, lambda *_: None)
                self.assertIs(caught.exception, problem)
            else:
                client_host.install(Path('/fixture/root'), Path('/fixture/home'), {}, None, lambda *_: None)
        return calls

    def test_toolchain_and_sdk_observation_follows_original_success_boundaries(self):
        self.assertEqual(self.installation_steps(), ['tools', 'toolchain_ready', 'profile', 'sdk', 'sdk_tools_ready', 'engine', 'ownership', 'release', 'device-setup'])
        self.assertEqual(self.installation_steps(bundled=True), ['tools', 'toolchain_ready', 'profile', 'sdk_tools_ready', 'engine', 'ownership', 'release', 'device-setup'])

    def test_failed_toolchain_or_sdk_keeps_original_error_and_does_not_claim_later_readiness(self):
        self.assertEqual(self.installation_steps('tools'), ['tools'])
        self.assertEqual(self.installation_steps('sdk'), ['tools', 'toolchain_ready', 'profile', 'sdk'])

    def test_existing_engine_emits_same_ready_phase_only_after_successful_query(self):
        import client_host
        with patch('container_host_runtime.selection', return_value=None), \
             patch.object(client_host.shutil, 'which', return_value='/fixture/docker'), \
             patch.object(client_host.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)) as query, \
             patch.object(kvm, 'emit_snapshot') as emit, \
             patch.object(client_host, 'activate_container_engine') as activate, \
             patch.dict(os.environ, {}, clear=True):
            client_host.container_engine(Path('/fixture'), Path('/fixture'), {}, lambda *_: None)
        query.assert_called_once_with(['docker', 'info'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        emit.assert_called_once_with('engine_ready')
        activate.assert_not_called()

    def test_preflight_emits_only_after_original_prerequisite_success(self):
        import linux_host
        self.assertEqual(Path(linux_host.__file__).resolve().parent, Path(__file__).resolve().parent)
        calls = []
        with patch.object(linux_host.Path, 'exists', return_value=True), \
             patch.object(linux_host, 'system_packages', return_value=[]), \
             patch.object(linux_host, 'install_dependencies', side_effect=lambda *_: calls.append('dependencies')), \
             patch.object(linux_host, 'require_kvm', side_effect=lambda: calls.append('require')), \
             patch.object(linux_host, 'emit_snapshot', side_effect=lambda phase: calls.append(phase)):
            linux_host.prepare_system(lambda *_: None)
        self.assertEqual(calls, ['dependencies', 'require', 'preflight_ready'])
        with patch.object(linux_host.Path, 'exists', return_value=True), \
             patch.object(linux_host, 'system_packages', return_value=[]), \
             patch.object(linux_host, 'install_dependencies'), \
             patch.object(linux_host, 'require_kvm', side_effect=linux_host.HostPrerequisite('virtualization_unavailable')), \
             patch.object(linux_host, 'emit_snapshot') as emit:
            with self.assertRaises(linux_host.HostPrerequisite):
                linux_host.prepare_system(lambda *_: None)
        emit.assert_not_called()

    def test_before_start_is_after_create_and_before_the_same_start_owner(self):
        import linux_host
        calls = []
        def start(*_):
            calls.append('start')
            raise RuntimeError('fixture stops before device readiness')
        with patch.object(linux_host, 'emit_snapshot', side_effect=lambda phase: calls.append(phase)):
            with self.assertRaisesRegex(RuntimeError, 'fixture stops'):
                linux_host.start_guest(Path('/fixture'), {}, lambda argv: calls.append(Path(argv[-1]).name), start)
        self.assertEqual(calls, ['start-server', 'android-create.sh', 'before_android_start', 'start'])

    def test_engine_observation_follows_completed_activation_and_retains_failure(self):
        import client_host
        self.assertEqual(Path(client_host.__file__).resolve().parent, Path(__file__).resolve().parent)
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            calls = []
            with patch('managed_engine.state_directory', return_value=home/'state'), \
                 patch('managed_engine.claim_state'), patch('managed_engine.save_selection', side_effect=lambda *_: calls.append('saved')), \
                 patch.object(client_host, 'write_container_wrapper', return_value=home/'bin'), \
                 patch.object(client_host.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, stdout='Running')), \
                 patch.object(client_host, 'execute', side_effect=lambda argv, **_: calls.append(str(argv[1]))), \
                 patch.object(kvm, 'emit_snapshot', side_effect=lambda phase: calls.append(phase)), \
                 patch.dict(os.environ, {'PATH': '/fixture'}, clear=True):
                client_host.activate_container_engine(home, home, home/'lima', {})
            self.assertEqual(calls, ['start', 'info', 'saved', 'engine_ready'])
            with patch('managed_engine.state_directory', return_value=home/'state'), \
                 patch('managed_engine.claim_state'), \
                 patch.object(client_host.subprocess, 'run', return_value=subprocess.CompletedProcess([], 7, stdout='')), \
                 patch.object(kvm, 'emit_snapshot') as emit, patch.dict(os.environ, {}, clear=True):
                with self.assertRaisesRegex(ValueError, 'state is unknown'):
                    client_host.activate_container_engine(home, home, home/'lima', {})
            emit.assert_not_called()


if __name__ == '__main__':
    unittest.main()
