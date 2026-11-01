"""Real start_unit resume guard, with process/identity boundaries narrowly mocked."""
from contextlib import ExitStack, redirect_stderr
import hashlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

if os.name != 'posix':
    raise unittest.SkipTest('Linux service receipt inspection requires POSIX nofollow semantics')

DENIED = ('subprocess.Popen', 'subprocess.run', 'subprocess.check_call',
          'subprocess.check_output', 'socket.socket', 'socket.create_connection',
          'os.system', 'os.kill', 'os.killpg')
with ExitStack() as import_guards:
    for boundary in DENIED:
        import_guards.enter_context(patch(boundary, side_effect=AssertionError('Unmocked import operation: ' + boundary)))
    import linux_services


class ResumeSpecTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='linux-resume-spec-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.folder = self.root / 'data/services'
        self.folder.mkdir(parents=True)
        self.service = 'emulator'
        self.command = ['sh', 'PRIVATE_ARGUMENT_DO_NOT_REPORT']
        self.environment = {'PATH': 'PRIVATE_PATH_DO_NOT_REPORT',
                            'JAVA_HOME': 'PRIVATE_JAVA_DO_NOT_REPORT'}
        self.old = {'command': self.command.copy(), 'environment': self.environment.copy()}
        self.receipt = {'format': 1, 'pid': 42, 'start_ticks': 'already-verified',
                        'root': str(self.root)}
        self.save_old()
        (self.folder / 'emulator.lock').touch(mode=0o600)
        self.output = io.StringIO()
        for boundary in DENIED:
            guard = patch(boundary, side_effect=AssertionError('Unmocked external operation: ' + boundary))
            guard.start()
            self.addCleanup(guard.stop)
        for boundary in ('linux_services.atomic_json', 'linux_services.threading.Thread',
                         'linux_services.time.sleep'):
            guard = patch(boundary, side_effect=AssertionError('Unexpected resume side effect: ' + boundary))
            guard.start()
            self.addCleanup(guard.stop)
        lock = patch('linux_services.fcntl.flock', side_effect=BlockingIOError)
        self.lock = lock.start()
        self.addCleanup(lock.stop)
        owner = patch('linux_services.unit_state', return_value=self.receipt)
        self.owner = owner.start()
        self.addCleanup(owner.stop)

    @property
    def spec_path(self):
        return self.folder / (self.service + '.spec.json')

    def save_old(self):
        self.spec_path.write_text(json.dumps(self.old, indent=2) + '\n')
        self.receipt['spec_sha256'] = hashlib.sha256(json.dumps(self.old, sort_keys=True).encode()).hexdigest()

    def invoke(self, *, command=None, environment=None):
        with redirect_stderr(self.output):
            return linux_services.start_unit(self.root, self.service,
                                            self.command if command is None else command,
                                            self.environment if environment is None else environment)

    def rejected(self, **options):
        with self.assertRaisesRegex(ValueError, '^A running task has a different service configuration; stop the owned installation before upgrading$'):
            self.invoke(**options)
        self.owner.assert_called_once_with(self.root, self.service)

    def report(self):
        lines = self.output.getvalue().splitlines()
        self.assertEqual(len(lines), 1)
        prefix = 'AI_PHONE_SERVICE_SPEC '
        self.assertTrue(lines[0].startswith(prefix))
        return json.loads(lines[0][len(prefix):])

    def assert_private(self):
        output = self.output.getvalue()
        for forbidden in ('PRIVATE_ARGUMENT_DO_NOT_REPORT', 'PRIVATE_PATH_DO_NOT_REPORT',
                          'PRIVATE_JAVA_DO_NOT_REPORT', 'PRIVATE_NEW_VALUE_DO_NOT_REPORT',
                          'PRIVATE_UNKNOWN_KEY_DO_NOT_REPORT', self.receipt['spec_sha256']):
            self.assertNotIn(forbidden, output)

    def assert_unverified(self):
        self.rejected(environment={**self.environment, 'PATH': 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
        self.assertEqual(self.report(), {'format': 1, 'service': 'emulator', 'evidence': 'unverified_spec'})
        self.assert_private()

    def test_identical_configuration_keeps_the_owned_unit_without_diagnostics(self):
        before = self.spec_path.read_bytes()
        with patch('linux_services.read_receipt', side_effect=AssertionError('Same spec must not inspect or emit a conflict')):
            self.assertFalse(self.invoke())
        self.owner.assert_called_once_with(self.root, self.service)
        self.assertEqual(self.spec_path.read_bytes(), before)
        self.assertEqual(self.output.getvalue(), '')

    def test_dictionary_insertion_order_does_not_change_existing_matching_contract(self):
        self.assertFalse(self.invoke(environment=dict(reversed(list(self.environment.items())))))
        self.assertEqual(self.output.getvalue(), '')

    def test_path_change_is_classified_without_values_and_still_rejected(self):
        before = self.spec_path.read_bytes()
        self.rejected(environment={**self.environment, 'PATH': 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
        self.assertEqual(self.report(), {'format': 1, 'service': 'emulator', 'evidence': 'verified_spec',
                                        'command_changed': False, 'environment_fields': ['PATH'],
                                        'other_environment_changes': 0})
        self.assertEqual(self.spec_path.read_bytes(), before)
        self.assert_private()

    def test_only_fixed_source_fields_can_be_reported_by_name(self):
        fields = ('PATH', 'JAVA_HOME', 'AI_PHONE_JAVA_HOME', 'AI_PHONE_SDK_ROOT',
                  'AI_PHONE_ADB', 'AI_PHONE_PROJECT_ROOT', 'AI_PHONE_DATA_ROOT')
        for field in fields:
            with self.subTest(field=field):
                self.output.seek(0)
                self.output.truncate(0)
                self.owner.reset_mock()
                self.rejected(environment={**self.environment, field: 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
                report = self.report()
                self.assertEqual(report['environment_fields'], [field])
                self.assertEqual(report['other_environment_changes'], 0)
                self.assertFalse(report['command_changed'])
                self.assert_private()

    def test_command_change_reports_a_boolean_without_argv(self):
        self.rejected(command=['sh', 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'])
        report = self.report()
        self.assertTrue(report['command_changed'])
        self.assertEqual(report['environment_fields'], [])
        self.assertEqual(report['other_environment_changes'], 0)
        self.assert_private()

    def test_unlisted_environment_add_remove_and_value_changes_are_counted_only(self):
        key = 'PRIVATE_UNKNOWN_KEY_DO_NOT_REPORT'
        cases = [({}, {key: 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'}),
                 ({key: 'PRIVATE_PATH_DO_NOT_REPORT'}, {}),
                 ({key: 'PRIVATE_PATH_DO_NOT_REPORT'}, {key: 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})]
        for old_extra, new_extra in cases:
            with self.subTest(old_present=bool(old_extra), new_present=bool(new_extra)):
                self.output.seek(0)
                self.output.truncate(0)
                self.owner.reset_mock()
                self.old['environment'] = {**self.environment, **old_extra}
                self.save_old()
                self.rejected(environment={**self.environment, **new_extra})
                report = self.report()
                self.assertEqual(report['environment_fields'], [])
                self.assertEqual(report['other_environment_changes'], 1)
                self.assertFalse(report['command_changed'])
                self.assert_private()

    def test_unknown_service_name_is_not_echoed(self):
        self.service = 'private-service-identifier'
        (self.folder / (self.service + '.lock')).touch(mode=0o600)
        self.save_old()
        self.rejected(environment={**self.environment, 'PATH': 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
        self.assertEqual(self.report()['service'], 'other')
        self.assertNotIn(self.service, self.output.getvalue())
        self.assert_private()

    def test_ai_phone_prefix_does_not_make_a_free_environment_key_reportable(self):
        key = 'AI_PHONE_PRIVATE_NAME_DO_NOT_REPORT'
        self.rejected(environment={**self.environment, key: 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
        self.assertEqual(self.report()['environment_fields'], [])
        self.assertEqual(self.report()['other_environment_changes'], 1)
        self.assertNotIn(key, self.output.getvalue())
        self.assert_private()

    def test_corrupt_stored_json_is_unverified_without_raw_errors(self):
        self.spec_path.write_text('{PRIVATE_PATH_DO_NOT_REPORT')
        self.assert_unverified()

    def test_oversized_stored_spec_is_unverified(self):
        self.spec_path.write_text(json.dumps(self.old) + ' ' * 8193)
        self.assert_unverified()

    def test_unmatched_stored_spec_digest_cannot_supply_differences(self):
        self.old['environment']['PATH'] = 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'
        self.spec_path.write_text(json.dumps(self.old))
        self.assert_unverified()

    def test_missing_stored_spec_is_unverified(self):
        self.spec_path.unlink()
        self.assert_unverified()

    def test_stored_spec_symlink_is_not_followed(self):
        foreign = self.root / 'foreign-spec.json'
        original = self.spec_path.read_bytes()
        foreign.write_bytes(original)
        self.spec_path.unlink()
        self.spec_path.symlink_to(foreign)
        self.assert_unverified()
        self.assertEqual(foreign.read_bytes(), original)

    def test_invalid_bound_spec_shapes_are_unverified(self):
        for value in ([], {'command': 'PRIVATE_ARGUMENT_DO_NOT_REPORT', 'environment': {}},
                      {'command': ['sh'], 'environment': []},
                      {'command': [None], 'environment': {}},
                      {'command': ['sh'], 'environment': {'PATH': None}}):
            with self.subTest(shape=type(value).__name__):
                self.output.seek(0)
                self.output.truncate(0)
                self.owner.reset_mock()
                self.old = value
                self.save_old()
                self.assert_unverified()

    def test_unexpected_reader_failure_preserves_original_configuration_rejection(self):
        with patch('linux_services.read_receipt', side_effect=RuntimeError('PRIVATE_PATH_DO_NOT_REPORT')):
            self.rejected(environment={**self.environment, 'PATH': 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
        self.assertEqual(self.output.getvalue(), '')

    def test_unexpected_classifier_failure_preserves_original_configuration_rejection(self):
        with patch('linux_services.spec_conflict_summary', side_effect=RuntimeError('PRIVATE_PATH_DO_NOT_REPORT')):
            self.rejected(environment={**self.environment, 'PATH': 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
        self.assertEqual(self.output.getvalue(), '')

    def test_failed_output_preserves_original_configuration_rejection(self):
        with patch('linux_services.print', create=True, side_effect=BrokenPipeError('PRIVATE_PATH_DO_NOT_REPORT')):
            self.rejected(environment={**self.environment, 'PATH': 'PRIVATE_NEW_VALUE_DO_NOT_REPORT'})
        self.assertEqual(self.output.getvalue(), '')


if __name__ == '__main__':
    unittest.main()
