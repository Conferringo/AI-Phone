"""Installer interruption/concurrency and service resume tests; no live guest writes."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
from install_state import InstallState,fingerprint
from service_start import SERVICES,start_missing
from service_stop import stop_owned

SCRIPTS=Path(__file__).resolve().parent

class InstallResumeTests(unittest.TestCase):
    def test_reuse_requires_same_source_and_valid_artifact(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'input').write_text('v1');artifact=root/'artifact';calls=[]
            def action():calls.append(1);artifact.write_text('valid')
            verify=lambda:artifact.is_file() and artifact.read_text()=='valid'
            with InstallState(root) as state:
                digest=fingerprint(root,['input']);self.assertTrue(state.phase('fixture',digest,action,verify))
                self.assertFalse(state.phase('fixture',digest,action,verify))
                artifact.write_text('corrupt');self.assertTrue(state.phase('fixture',digest,action,verify))
                (root/'input').write_text('v2');self.assertTrue(state.phase('fixture',fingerprint(root,['input']),action,verify))
            self.assertEqual(len(calls),3);self.assertEqual((root/'data/deployment/install-state.json').stat().st_mode&0o777,0o600)
    def test_changed_artifact_identity_forces_revalidation(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);artifact=root/'image-id';calls=[]
            def action():calls.append(1);artifact.write_text('expected-image')
            verify=lambda:artifact.read_text() if artifact.exists() else ''
            with InstallState(root) as state:
                state.phase('image','same',action,verify)
                artifact.write_text('replaced-image')
                self.assertTrue(state.phase('image','same',action,verify))
            self.assertEqual(len(calls),2)

    def test_process_kill_releases_lock_and_retries_only_incomplete_phase(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            code='''import sys,time
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from install_state import InstallState
root=Path(sys.argv[2])
with InstallState(root) as state:
 def first():
  path=root/'first-count';path.write_text(str(int(path.read_text())+1) if path.exists() else '1')
 state.phase('first','same',first,lambda:(root/'first-count').is_file())
 def second():
  (root/'entered').write_text('ready')
  if sys.argv[3]=='hold':time.sleep(60)
  (root/'finished').write_text('complete')
 state.phase('second','same',second,lambda:(root/'finished').exists())
'''
            with (root/'child.log').open('w') as log:
                p=subprocess.Popen([sys.executable,'-c',code,str(SCRIPTS),str(root),'hold'],stdout=log,stderr=log)
                try:
                    deadline=time.monotonic()+10
                    while not (root/'entered').exists() and time.monotonic()<deadline:time.sleep(.05)
                    self.assertTrue((root/'entered').exists())
                    with self.assertRaisesRegex(ValueError,'Another installer'):
                        with InstallState(root):pass
                    p.kill();p.wait(timeout=5)
                finally:
                    if p.poll() is None:p.kill();p.wait(timeout=5)
            subprocess.run([sys.executable,'-c',code,str(SCRIPTS),str(root),'continue'],check=True,capture_output=True)
            self.assertEqual((root/'first-count').read_text(),'1');state=json.loads((root/'data/deployment/install-state.json').read_text());self.assertEqual(state['phases']['second']['attempt'],2);self.assertEqual(state['phases']['second']['state'],'completed')
    def test_failed_or_interrupted_phase_does_not_complete(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            for error,label in [(ValueError('fixture secret should not be saved'),'failed'),(KeyboardInterrupt(),'interrupted')]:
                with self.subTest(label=label), InstallState(root) as state:
                    def fail():raise error
                    with self.assertRaises(type(error)):state.phase('fixture','hash',fail,lambda:True)
                    self.assertEqual(state.state['phases']['fixture']['state'],label)
                    self.assertNotIn('fixture secret',(root/'data/deployment/install-state.json').read_text())

class LaunchdFixture(unittest.TestCase):
    def setUp(self):
        # These fixtures exercise launchd contracts even on a Linux CI runner.
        host=patch('platform.system',return_value='Darwin');host.start();self.addCleanup(host.stop)

class ServiceResumeTests(LaunchdFixture):
    def test_loaded_but_stopped_job_is_started_without_killing_live_work(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);calls=[]
            def query(label):
                state='waiting' if label=='dev.aiphone.web.p2' else 'running'
                return subprocess.CompletedProcess([],0,stdout=f'working directory = {root}\nstate = {state}\n')
            with patch('service_start.subprocess.run') as native:
                result=start_missing(root,query=query,execute=calls.append,package_installed=lambda _:True)
            self.assertEqual(result,['dev.aiphone.web.p2'])
            self.assertEqual(calls,[], 'a registered stopped job needs no rebuild or new configuration')
            native.assert_called_once_with(['launchctl','kickstart',f'gui/{os.getuid()}/dev.aiphone.web.p2'],check=True)
    def test_partial_install_only_starts_missing_jobs_and_missing_app(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);calls=[];missing={'dev.aiphone.web.p2','dev.aiphone.lan'}
            def query(label):return subprocess.CompletedProcess([],1 if label in missing else 0,stdout='working directory = '+str(root)+'\nstate = running\n')
            result=start_missing(root,query=query,execute=calls.append,package_installed=lambda package:package!='dev.aiphone.input')
            self.assertEqual(calls,['android-ready.sh','android-install-input.sh','phone-web-start.sh','lan-start.sh']);self.assertEqual(set(result),missing)
    def test_existing_instance_is_not_restarted_and_foreign_instance_refused_first(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);calls=[]
            query=lambda label:subprocess.CompletedProcess([],0,stdout='working directory = '+str(root)+'\nstate = running\n')
            self.assertEqual(start_missing(root,query=query,execute=calls.append,package_installed=lambda _:True),[]);self.assertEqual(calls,[])
            foreign=lambda label:subprocess.CompletedProcess([],0,stdout='working directory = '+str(root)+'-other\n')
            with self.assertRaisesRegex(ValueError,'Another checkout'):start_missing(root,query=foreign,execute=calls.append)
            self.assertEqual(calls,[])
    def test_missing_emulator_boots_before_packages_and_quarantine_blocks_everything(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);calls=[];query=lambda _:subprocess.CompletedProcess([],1,stdout='')
            start_missing(root,query=query,execute=calls.append,package_installed=lambda _:True)
            self.assertEqual(calls[:3],['android-create.sh','android-start.sh','android-ready.sh'])
            (root/'data').mkdir(exist_ok=True);(root/'data/restore-quarantine.json').write_text('{}');calls.clear()
            with self.assertRaisesRegex(ValueError,'quarantined'):start_missing(root,query=query,execute=calls.append)
            self.assertEqual(calls,[])

class ServiceStopTests(LaunchdFixture):
    def test_stop_refuses_foreign_jobs_before_any_change(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);calls=[]
            foreign=lambda label:subprocess.CompletedProcess([],0,stdout='working directory = '+str(root)+'-foreign\n')
            with self.assertRaisesRegex(ValueError,'Another checkout'):stop_owned(root,query=foreign,execute=calls.append)
            self.assertEqual(calls,[])
    def test_missing_emulator_does_not_kill_an_unowned_instance(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);calls=[]
            def query(label):return subprocess.CompletedProcess([],0 if label=='dev.aiphone.web.p2' else 1,stdout='working directory = '+str(root)+'\n')
            stop_owned(root,query=query,execute=calls.append)
            self.assertEqual(len(calls),1);self.assertIn('dev.aiphone.web.p2',calls[0][-1])

if __name__=='__main__':unittest.main()
