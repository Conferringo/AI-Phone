"""Verification children cannot inherit cache policy or credentials from a host."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from runtime_verification import isolated_environment


class RuntimeVerificationEnvironmentTests(unittest.TestCase):
    def test_bun_runtime_cache_is_disabled_without_broadening_environment(self):
        inherited = {'HOME':'/owned-fixture-home', 'USERPROFILE':'C:/owned-fixture-home',
                     'SYSTEMROOT':'C:/Windows', 'LANG':'C',
                     'BUN_RUNTIME_TRANSPILER_CACHE_PATH':'/private-cache',
                     'MODEL_KEY':'fixture-only', 'DOCKER_HOST':'unix:///private-socket',
                     'NODE_OPTIONS':'--require private-module', 'XDG_CACHE_HOME':'/private-xdg'}
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, inherited, clear=True):
            root = Path(temporary)
            environment = isolated_environment(root)
            self.assertEqual(environment['BUN_RUNTIME_TRANSPILER_CACHE_PATH'], '0')
            self.assertEqual(environment, {'HOME':inherited['HOME'], 'USERPROFILE':inherited['USERPROFILE'],
                'SYSTEMROOT':inherited['SYSTEMROOT'], 'LANG':'C', 'PATH':os.defpath,
                'AI_PHONE_PROJECT_ROOT':str(root.resolve()), 'BUN_RUNTIME_TRANSPILER_CACHE_PATH':'0'})
            self.assertEqual(list(root.iterdir()), [])

    def test_private_input_guards_are_preserved(self):
        for name in ('.env','data/account-fixture'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);state=root/name;state.parent.mkdir(parents=True,exist_ok=True)
                state.write_text('fixture-only')
                with self.assertRaisesRegex(ValueError,'isolated runtime'):
                    isolated_environment(root)
                self.assertEqual(state.read_text(),'fixture-only')


if __name__=='__main__':unittest.main()
