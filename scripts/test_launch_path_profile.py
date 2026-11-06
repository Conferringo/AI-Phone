"""Real profile/configuration/PATH contracts; process and network operations denied."""
from contextlib import ExitStack, redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

DENIED = ('subprocess.Popen', 'subprocess.run', 'subprocess.check_call',
          'subprocess.check_output', 'socket.socket', 'socket.create_connection',
          'urllib.request.build_opener', 'os.system', 'os.kill', 'os.killpg')
with ExitStack() as guards:
    for boundary in DENIED:
        guards.enter_context(patch(boundary, side_effect=AssertionError('Unmocked import operation: ' + boundary)))
    import client_host
    import release
    import runtime_config
    if os.name == 'posix':
        import linux_services


class LaunchPathTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='launch-path-profile-')
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name).resolve()
        self.root = self.home / 'releases' / ('a' * 64)
        self.root.mkdir(parents=True)
        self.source = self.home / 'configuration.json'
        self.env_file = self.root / '.env'
        self.env_bytes = b'POSTGRES_PASSWORD=PRIVATE_DATABASE_VALUE\n'
        self.env_file.write_bytes(self.env_bytes)
        self.profile = self.root / 'data/deployment/profile.json'
        self.path_a = os.pathsep.join([str(self.home / 'managed toolchain/bin'), '/usr/bin', '/bin'])
        self.path_b = os.pathsep.join([str(self.home / 'different caller/bin'), '/usr/bin', '/bin'])
        self.request = {'locale': 'en', 'phone': {'cpus': 2, 'memory_mb': 2048, 'storage_gb': 16},
                        'desktop': {'cpus': 2, 'memory_mb': 2048}}
        self.host = {'phone_template': 'android35-x86_64'}
        environment = patch.dict(os.environ, {'PATH': self.path_a, 'JAVA_HOME': str(self.home / 'java'),
                                              'AI_PHONE_SDK_ROOT': str(self.home / 'tools/android-sdk')}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)
        for boundary in DENIED:
            guard = patch(boundary, side_effect=AssertionError('Unmocked external operation: ' + boundary))
            guard.start()
            self.addCleanup(guard.stop)

    def prepare(self, request=None):
        with redirect_stdout(io.StringIO()):
            client_host.prepare_profile(self.root, self.home, self.request if request is None else request, self.host)
        return json.loads(self.profile.read_text())

    def configure(self, value):
        self.source.write_text(json.dumps(value))
        with redirect_stdout(io.StringIO()):
            release.configure(self.root, self.source)

    def test_new_profile_captures_tool_install_environment_then_ignores_caller_path(self):
        mise = self.home / 'tools/bin/mise'
        tools = ['python@3.13.18', 'node@22.23.4']
        selected = os.pathsep.join([str(self.home / 'python/bin'), str(self.home / 'node/bin'), '/usr/bin', '/bin'])
        commands = []
        def execute(argv, **options):
            commands.append(list(map(str, argv)))
            self.assertEqual(options['cwd'], self.home)
            if commands[-1] == [str(mise), 'install', '-y', *tools]:
                return subprocess.CompletedProcess(argv, 0)
            if commands[-1] == [str(mise), 'env', '--json', *tools]:
                return subprocess.CompletedProcess(argv, 0, stdout=json.dumps({'PATH': selected}))
            raise AssertionError('Unexpected mocked installer operation')
        with patch('client_host.execute', side_effect=execute):
            client_host.install_tools(self.home, mise, tools, lambda *_: None)
        value = self.prepare()
        self.assertEqual(value['launch_path'], selected)
        before = runtime_config.runtime_env(self.root)
        os.environ['PATH'] = self.path_b
        after = runtime_config.runtime_env(self.root)
        self.assertEqual(after, before)
        self.assertEqual(len(commands), 2)
        self.assertEqual(self.env_file.read_bytes(), self.env_bytes)
        self.assertEqual(value['phone'], {'template': 'android35-x86_64', **self.request['phone']})
        self.assertEqual(value['desktop'], {'template': 'debian13-openbox', **self.request['desktop']})

    def test_existing_owned_profile_and_resources_are_not_rewritten_on_resume(self):
        value = self.prepare()
        before = self.profile.read_bytes()
        os.environ['PATH'] = self.path_b
        changed = {'locale': 'de', 'phone': {'cpus': 4, 'memory_mb': 4096, 'storage_gb': 32},
                   'desktop': {'cpus': 4, 'memory_mb': 4096}}
        self.assertEqual(self.prepare(changed), value)
        self.assertEqual(self.profile.read_bytes(), before)
        self.assertEqual(self.env_file.read_bytes(), self.env_bytes)

    def test_legacy_profile_is_not_silently_given_a_launch_path(self):
        value = self.prepare()
        value.pop('launch_path')
        self.configure(value)
        before = self.profile.read_bytes()
        first = runtime_config.runtime_env(self.root)['PATH']
        os.environ['PATH'] = self.path_b
        self.prepare()
        second = runtime_config.runtime_env(self.root)['PATH']
        self.assertNotEqual(first, second, 'Legacy fallback remains legacy; no full-upgrade claim')
        self.assertEqual(self.profile.read_bytes(), before)
        self.assertNotIn('launch_path', json.loads(before))
        self.assertEqual(self.env_file.read_bytes(), self.env_bytes)

    def test_new_default_configuration_also_has_a_path_owner(self):
        value = release.default_profile(self.root)
        self.assertIsInstance(value['launch_path'], str)
        self.configure(value)
        before = runtime_config.runtime_env(self.root)['PATH']
        os.environ['PATH'] = self.path_b
        self.assertEqual(runtime_config.runtime_env(self.root)['PATH'], before)

    def test_launch_path_validation_is_shared_at_write_and_read_boundaries(self):
        value = self.prepare()
        invalid = (None, 1, True, [], ['/usr/bin'], {}, '', '   ', '/a\0/b', '/a\n/b',
                   '/a\t/b', '/a\r/b', '/a\x7f/b', '/a\x85/b')
        for path in invalid:
            with self.subTest(kind=type(path).__name__):
                changed = {**value, 'launch_path': path}
                with self.assertRaisesRegex(ValueError, 'launch_path'):
                    release.validate(changed)
                self.profile.write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, 'launch_path'):
                    runtime_config.runtime_env(self.root)

    def test_unicode_and_spaces_are_preserved(self):
        value = self.prepare()
        value['launch_path'] = os.pathsep.join([str(self.home / '工具/bin'), str(self.home / 'my tools/bin')])
        self.configure(value)
        paths = runtime_config.runtime_env(self.root)['PATH'].split(os.pathsep)
        self.assertIn(str(self.home / '工具/bin'), paths)
        self.assertIn(str(self.home / 'my tools/bin'), paths)

    def test_path_text_is_never_shell_expanded(self):
        value = self.prepare()
        literal = ['$HOME/literal', '$(not-a-command)', '~/literal']
        value['launch_path'] = os.pathsep.join(literal)
        self.configure(value)
        paths = runtime_config.runtime_env(self.root)['PATH'].split(os.pathsep)
        self.assertEqual(paths[-3:], literal)

    def test_private_tool_and_engine_prefix_order_is_preserved(self):
        self.prepare()
        tool_paths = [str(self.home / 'bundled-python/bin'), str(self.home / 'bundled-node/bin')]
        engine = str(self.home / 'engine/bin')
        def tools(_root, base):
            return {'PATH': os.pathsep.join([*tool_paths, base]), 'AI_PHONE_PYTHON': str(self.home / 'bundled-python/bin/python')}
        with patch('installer_tools.selected_environment', side_effect=tools), patch('managed_engine.environment', return_value={'AI_PHONE_MANAGED_BIN': engine}):
            first = runtime_config.runtime_env(self.root)
            os.environ['PATH'] = self.path_b
            self.assertEqual(runtime_config.runtime_env(self.root)['PATH'], first['PATH'])
        self.assertEqual(first['PATH'].split(os.pathsep)[:4], [engine, *tool_paths, str(self.root / '.tools/bin')])

    def test_old_service_spec_cannot_supply_the_path_owner(self):
        self.prepare()
        spec = self.root / 'data/services/emulator.spec.json'
        spec.parent.mkdir(parents=True)
        spec.write_text('{PRIVATE_OLD_SPEC_NOT_CONFIGURATION')
        first = runtime_config.runtime_env(self.root)['PATH']
        spec.write_text('DIFFERENT_PRIVATE_OLD_SPEC')
        os.environ['PATH'] = self.path_b
        self.assertEqual(runtime_config.runtime_env(self.root)['PATH'], first)

    def test_same_profile_keeps_launch_environment_and_real_profile_changes_update_it(self):
        value = self.prepare()
        first = runtime_config.runtime_env(self.root)
        os.environ['PATH'] = self.path_b
        second = runtime_config.runtime_env(self.root)
        self.assertEqual(first,second)
        value['phone']['memory_mb'] = 4096
        self.configure(value)
        changed = runtime_config.runtime_env(self.root)
        self.assertNotEqual(changed['AI_PHONE_PHONE_MEMORY'],first['AI_PHONE_PHONE_MEMORY'])
        value['phone']['memory_mb'] = 2048
        value['launch_path'] = str(self.home / 'explicitly changed owner/bin')
        self.configure(value)
        self.assertNotEqual(runtime_config.runtime_env(self.root)['PATH'],first['PATH'])
        self.assertEqual(self.env_file.read_bytes(), self.env_bytes)


if __name__ == '__main__':
    unittest.main()
