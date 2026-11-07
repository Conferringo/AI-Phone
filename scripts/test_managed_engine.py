"""Installer integration with real Rust policy; detailed state cases live in Rust."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from test_release import native_profile_program


class ManagedEngineTests(unittest.TestCase):
    def test_saved_native_environment_is_used_by_runtime_and_restart(self):
        from managed_engine import state_directory, claim_state, save_selection, environment, ensure_running
        from client_host import write_container_wrapper
        from runtime_config import runtime_env
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary).resolve();home=base/'installation';data=home/'user/data';data.mkdir(parents=True)
            root=home/'releases/a';root.mkdir(parents=True);(root/'data').symlink_to(data,target_is_directory=True)
            command=home/'limactl';command.write_text('#!/bin/sh\nprintf started > "$LIMA_HOME/started"\n');command.chmod(0o755)
            with patch('managed_engine.Path.home',return_value=base),patch('prebuilt_services.managed_engine_command',return_value=[native_profile_program()]),patch('prebuilt_services.runtime_environment_command',return_value=[native_profile_program()]):
                state=state_directory(root,home);claim_state(root,state);wrapper=write_container_wrapper(root,home,command,state)
                save_selection(root,home,state,command,wrapper)
                self.assertEqual(environment(root),{'AI_PHONE_MANAGED_BIN':str(wrapper),'LIMA_HOME':str(state)})
                with patch.dict(os.environ,{'PATH':'/usr/bin:/bin'},clear=True):
                    self.assertEqual(runtime_env(root)['PATH'].split(os.pathsep)[0],str(wrapper))
                self.assertTrue(ensure_running(root));self.assertEqual((state/'started').read_text(),'started')
                (wrapper/'docker').write_text('changed')
                with self.assertRaisesRegex(ValueError,'managed_engine_executable_changed'):ensure_running(root)

    def test_absent_selection_does_not_start_a_helper_or_touch_the_host_engine(self):
        from managed_engine import selection, environment, ensure_running
        with tempfile.TemporaryDirectory() as temporary,patch('prebuilt_services.managed_engine_command') as command:
            root=Path(temporary)
            self.assertIsNone(selection(root));self.assertEqual(environment(root),{});self.assertFalse(ensure_running(root))
            command.assert_not_called();self.assertEqual(list(root.iterdir()),[])
