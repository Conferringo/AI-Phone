"""Windows precompile, packaging and post-build tests must share one Cargo profile."""
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import build_client


class NativeClientCargoTests(unittest.TestCase):
    def test_windows_build_and_test_reuse_the_installers_target_and_remapped_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'config').mkdir()
            (root / 'config/release-policy.json').write_text(json.dumps({'version': 'fixture'}))
            source = root / 'apps/client/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/AI Phone_fixture_x64-setup.exe'
            source.parent.mkdir(parents=True)
            source.write_bytes(b'fixture')
            with patch.object(build_client, 'ROOT', root), patch.object(build_client.platform, 'system', return_value='Windows'), \
                    patch.object(build_client.subprocess, 'run') as run, \
                    patch.object(build_client, 'inspect_windows', return_value={}), \
                    patch.object(build_client, 'record', return_value={'server_included': True, 'complete_device_bundle': True}), \
                    redirect_stdout(io.StringIO()):
                build_client.windows_cargo('build')
                build_client.build_other('windows')
                build_client.windows_cargo('test')
            self.assertEqual(run.call_count, 3)
            calls = run.call_args_list
            self.assertEqual(calls[0].args[0][0:2], ['cargo', 'build'])
            self.assertEqual(calls[2].args[0][0:2], ['cargo', 'test'])
            for call in calls:
                arguments = call.args[0]
                self.assertEqual(arguments[arguments.index('--target') + 1], 'x86_64-pc-windows-msvc')
                self.assertIn('--locked', arguments)
                self.assertTrue(call.kwargs['check'])
                self.assertEqual(call.kwargs['env']['CARGO_ENCODED_RUSTFLAGS'], calls[1].kwargs['env']['CARGO_ENCODED_RUSTFLAGS'])
                self.assertEqual(call.kwargs['env']['CARGO_INCREMENTAL'], '0')
            self.assertIn('--release', calls[0].args[0])
            self.assertIn('--release', calls[2].args[0])
            self.assertIn('--bins', calls[0].args[0])
            self.assertNotIn('--bins', calls[2].args[0])
            for call in (calls[0], calls[2]):
                arguments=call.args[0]
                self.assertEqual(arguments[arguments.index('--features')+1], 'tauri/custom-protocol')

    def test_precompile_failure_is_returned_without_retry(self):
        failure = subprocess.CalledProcessError(7, ['cargo', 'build'])
        with patch.object(build_client.subprocess, 'run', side_effect=failure) as run:
            with self.assertRaises(subprocess.CalledProcessError) as raised:
                build_client.windows_cargo('build')
        self.assertIs(raised.exception, failure)
        run.assert_called_once()


if __name__ == '__main__':
    unittest.main()
