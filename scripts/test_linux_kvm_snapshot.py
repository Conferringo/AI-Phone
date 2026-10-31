"""Fixed-device diagnostic contracts; no host device is accessed."""
import os
import unittest

if os.name != 'posix':
    raise unittest.SkipTest('POSIX fixed-device diagnostic fixtures')

import contextlib
import errno
import io
import json
import stat
import subprocess
import tempfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

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
        handles = {
            'stat': mocked('linux_kvm_snapshot.os.lstat', return_value=info),
            'access': mocked('linux_kvm_snapshot.os.access', return_value=True),
            'open': mocked('linux_kvm_snapshot.os.open', return_value=73),
            'fstat': mocked('linux_kvm_snapshot.os.fstat', return_value=info),
            'close': mocked('linux_kvm_snapshot.os.close'),
            'ioctl': mocked('fcntl.ioctl', return_value=12),
        }
        yield handles


class KvmObservationTests(unittest.TestCase):
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
        self.assertEqual(set(value), {'platform', 'override_present', 'identity', 'device', 'access', 'ioctl'})
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


class CheckpointConsumerTests(unittest.TestCase):
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
