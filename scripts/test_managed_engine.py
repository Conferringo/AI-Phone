"""Owned container state survives client restarts without global Docker changes."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


class ManagedEngineTests(unittest.TestCase):
    def test_bad_owner_socket_path_and_existing_state_are_preserved(self):
        from managed_engine import claim_state,state_directory,installation_home,verify_state
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary);home,root=self.layout(base)
            with self.assertRaises(ValueError):installation_home(base)
            with patch('managed_engine.Path.home',return_value=base/('long'*30)),self.assertRaisesRegex(ValueError,'too long'):state_directory(root,home)
            with patch('managed_engine.Path.home',return_value=base):
                state=state_directory(root,home);claim_state(root,state);marker=state/'.aiphone-owner.json';marker.write_text(json.dumps({'format':1,'data_root':'different'}))
                with self.assertRaises(ValueError):verify_state(root,state)
                marker.unlink();(state/'unknown').write_text('keep')
                with self.assertRaises(ValueError):claim_state(root,state)
                self.assertEqual((state/'unknown').read_text(),'keep')

    def layout(self,base):
        home=base/'installation';data=home/'user/data';data.mkdir(parents=True)
        root=home/'releases'/('a'*64);root.mkdir(parents=True);(root/'data').symlink_to(data,target_is_directory=True)
        return home,root

    def test_state_is_short_private_and_bound_to_user_data_not_release(self):
        from managed_engine import state_directory,claim_state
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary);home,root=self.layout(base)
            with patch('managed_engine.Path.home',return_value=base):
                state=state_directory(root,home);claim_state(root,state)
                self.assertLess(len(os.fsencode(str(state/'ai-phone/ssh.sock.1234567890123456'))),104)
                self.assertEqual(state.stat().st_mode&0o777,0o700)
                (state/'keep').write_text('device-state');claim_state(root,state)
                other=home/'releases'/('b'*64);other.mkdir();(other/'data').symlink_to(home/'user/data',target_is_directory=True)
                self.assertEqual(state_directory(other,home),state)
                self.assertEqual((state/'keep').read_text(),'device-state')

    def test_foreign_symlink_and_unowned_state_never_get_adopted(self):
        from managed_engine import claim_state
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary);home,root=self.layout(base);foreign=base/'state';foreign.mkdir();(foreign/'unknown').write_text('keep')
            with self.assertRaises(ValueError):claim_state(root,foreign)
            self.assertEqual((foreign/'unknown').read_text(),'keep');self.assertEqual(foreign.stat().st_mode&0o777,0o755)
            link=base/'alias';link.symlink_to(foreign,target_is_directory=True)
            with self.assertRaises(ValueError):claim_state(root,link)

    def test_saved_wrapper_environment_is_restored_and_tampering_fails(self):
        from managed_engine import state_directory,claim_state,save_selection,environment,ensure_running
        from client_host import write_container_wrapper
        from runtime_config import runtime_env
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary);home,root=self.layout(base);command=home/'limactl';command.write_text('#!/bin/sh\nexit 0\n');command.chmod(0o755)
            with patch('managed_engine.Path.home',return_value=base):
                state=state_directory(root,home);claim_state(root,state)
                wrapper=write_container_wrapper(root,home,command,state)
                save_selection(root,home,state,command,wrapper)
                self.assertEqual(environment(root),{'AI_PHONE_MANAGED_BIN':str(wrapper.resolve()),'LIMA_HOME':str(state)})
                with patch.dict(os.environ,{'PATH':'/usr/bin:/bin'},clear=True):
                    self.assertEqual(runtime_env(root)['PATH'].split(os.pathsep)[0],str(wrapper.resolve()))
                with patch('managed_engine.subprocess.run',return_value=subprocess.CompletedProcess([],0)) as run:
                    self.assertTrue(ensure_running(root));self.assertEqual(run.call_args.args[0],[str(command),'start','--tty=false','ai-phone'])
                record=root/'data/deployment/container-host.json';saved=json.loads(record.read_text())
                for name in ('command','command_sha256','wrapper_sha256'):
                    record.write_text(json.dumps({**saved,name:None}))
                    with self.assertRaises(ValueError):environment(root)
                for extra in ({'format':True},{'format':2},{'home':'different'},{'data_root':'different'},{'state':'different'},{'command_sha256':'invalid'},{'extra':'unexpected'},{'command':123}):
                    record.write_text(json.dumps({**saved,**extra}))
                    with self.assertRaises(ValueError):environment(root)
                record.write_text(json.dumps(saved))
                command.chmod(0o644)
                with self.assertRaises(ValueError):environment(root)
                command.chmod(0o755)
                (wrapper/'docker').write_text('changed')
                with self.assertRaises(ValueError):environment(root)
                with self.assertRaises(ValueError):ensure_running(root)

    def test_lost_state_is_not_silently_recreated_during_environment_read(self):
        from managed_engine import state_directory,claim_state,save_selection,environment
        from client_host import write_container_wrapper
        import shutil
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary);home,root=self.layout(base);command=home/'limactl';command.write_text('#!/bin/sh\nexit 0\n');command.chmod(0o755)
            with patch('managed_engine.Path.home',return_value=base):
                state=state_directory(root,home);claim_state(root,state);wrapper=write_container_wrapper(root,home,command,state);save_selection(root,home,state,command,wrapper)
                shutil.rmtree(state)
                with self.assertRaises(ValueError):environment(root)
                self.assertFalse(state.exists())

    def test_absent_selection_leaves_existing_host_engine_untouched(self):
        from managed_engine import environment,ensure_running
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            with patch('managed_engine.subprocess.run') as run:
                self.assertEqual(environment(root),{});self.assertFalse(ensure_running(root));run.assert_not_called()

    def test_socket_and_record_boundaries_and_failed_start(self):
        from managed_engine import socket_budget,state_directory,claim_state,save_selection,environment,ensure_running,validate_directory
        from client_host import write_container_wrapper
        suffix=len(os.fsencode('/ai-phone/ssh.sock.1234567890123456'));socket_budget(Path('a'*(103-suffix)))
        with self.assertRaises(ValueError):socket_budget(Path('a'*(104-suffix)))
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary);home,root=self.layout(base);command=home/'limactl';command.write_text('#!/bin/sh\nexit 9\n');command.chmod(0o755)
            with patch('managed_engine.Path.home',return_value=base):
                state=state_directory(root,home);claim_state(root,state);wrapper=write_container_wrapper(root,home,command,state)
                with self.assertRaises(ValueError):save_selection(root,home,state.parent/'wrong',command,wrapper)
                self.assertFalse((state.parent/'wrong').exists());save_selection(root,home,state,command,wrapper)
                with self.assertRaises(subprocess.CalledProcessError):ensure_running(root)
                record=root/'data/deployment/container-host.json';text=record.read_text();record.write_text(text+' '*(16384-len(text)));self.assertTrue(environment(root));record.write_text(record.read_text()+' ')
                with self.assertRaises(ValueError):environment(root)
                record.write_text(text);target=home/'record-copy';target.write_text(text);record.unlink();record.symlink_to(target)
                with self.assertRaises(ValueError):environment(root)
                alias=state.parent/'alias';alias.symlink_to(state,target_is_directory=True)
                with self.assertRaises(ValueError):validate_directory(alias)


if __name__=='__main__':unittest.main()
