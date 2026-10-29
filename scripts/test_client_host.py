"""Native-worker contracts using isolated files and real fixture processes."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from client_host import check_request, sdk_licenses, verify_container_ownership, installation_reserve
from install_state import InstallState


class NativeHostTests(unittest.TestCase):
    def test_toolchain_interruption_reuses_only_matching_native_admission(self):
        with tempfile.TemporaryDirectory() as temp:
            home=Path(temp);root=home/'releases'/('a'*64);root.mkdir(parents=True)
            config={'minimum_free_gb':35,'minimum_resume_free_gb':2}
            self.assertEqual(installation_reserve(root,home,config),35)
            receipt=home/'deployment.json'
            receipt.write_text(json.dumps({'format':1,'payload':root.name,'progress':{'state':'running'}}))
            self.assertEqual(installation_reserve(root,home,config),2)
            receipt.write_text(json.dumps({'format':1,'payload':'b'*64,'progress':{'state':'running'}}))
            self.assertEqual(installation_reserve(root,home,config),35)
            receipt.write_text('partial receipt')
            self.assertEqual(installation_reserve(root,home,config),35)
    def test_source_profile_cannot_inject_a_shell_path_or_unbounded_resources(self):
        value = dict(locale='en', phone=dict(cpus=2, memory_mb=2048, storage_gb=16), desktop=dict(cpus=2, memory_mb=2560))
        self.assertEqual(check_request(value), value)
        for bad in [{**value, 'command': 'unsafe'}, {**value, 'locale': '../../'}, {**value, 'phone': {**value['phone'], 'cpus': True}}, {**value, 'desktop': {**value['desktop'], 'memory_mb': 1000000}}]:
            with self.assertRaises(ValueError): check_request(bad)

    def test_license_requires_each_actual_prompt_and_never_automatically_accepts(self):
        with tempfile.TemporaryDirectory() as temp:
            manager = Path(temp) / 'sdkmanager'
            manager.write_text('#!' + sys.executable + '\nimport sys\nfor text in ["Review licenses (y/N)?", "Fixture license text. Accept? (y/N):"]:\n print(text, end="", flush=True)\n if input() != "y":sys.exit(2)\n')
            manager.chmod(0o755); events=[]
            def emit(phase, state='running', **values): events.append((phase, state, values))
            with patch('sys.stdin', io.StringIO('{"accept":true}\n{"accept":true}\n')):
                sdk_licenses(manager, None, emit)
            licenses=[values['license'] for _,state,values in events if state=='license']
            self.assertEqual(len(licenses),2);self.assertIn('Fixture license text',licenses[1])
            with patch('sys.stdin', io.StringIO('')):
                with self.assertRaisesRegex(ValueError, 'not accepted'): sdk_licenses(manager, None, emit)

    def test_foreign_database_is_refused_before_compose_can_change_anything(self):
        root=Path('/owned')
        def result(owner): return subprocess.CompletedProcess([],0,stdout=json.dumps({'com.docker.compose.project':'ai-phone','com.docker.compose.project.working_dir':owner}))
        verify_container_ownership(root,lambda _:result('/owned'))
        with self.assertRaisesRegex(ValueError, 'Another installation'): verify_container_ownership(root,lambda _:result('/somebody-else'))

    def test_progress_observer_preserves_checkpoint_order_and_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            events=[]
            with InstallState(Path(temp),lambda phase,state:events.append((phase,state))) as install:
                with self.assertRaises(ValueError): install.phase('fixture','input',lambda:None,lambda:False)
                self.assertEqual(install.state['phases']['fixture']['state'],'failed')
            self.assertEqual(events,[('fixture','running'),('fixture','failed')])


if __name__ == '__main__': unittest.main()
