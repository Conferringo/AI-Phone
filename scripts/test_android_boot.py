import contextlib
import io
import fcntl
import json
import os
from pathlib import Path
import subprocess
import signal
import time
import sys
import tempfile
import unittest
from unittest.mock import patch

import android_boot as boot
import android_boot_diagnostics as diag


def result(stdout='', code=0, stderr=''):
    return subprocess.CompletedProcess([], code, stdout, stderr)


class Clock:
    def __init__(self): self.value = 0
    def now(self): return self.value
    def sleep(self, seconds): self.value += seconds


class AndroidBootTests(unittest.TestCase):
    def test_offline_failure_keeps_120_seconds_and_never_modifies_guest(self):
        clock = Clock(); calls = []; state = {'observations': 0, 'last_boot': {}}
        def invoke(argv, timeout):
            calls.append((argv, timeout)); return result(code=1, stderr='error: device offline')
        with self.assertRaisesRegex(boot.BootFailure, 'within 120 seconds') as caught:
            boot.ready(['owned-adb'], invoke=invoke, clock=clock.now, sleep=clock.sleep, state=state)
        self.assertEqual(caught.exception.phase, 'boot_failed')
        self.assertEqual(clock.value, 120); self.assertEqual(len(calls), 120)
        self.assertEqual(state['last_boot']['return_code'], 1)
        self.assertTrue(all(argv == ['owned-adb','shell','getprop','sys.boot_completed'] and timeout == 6 for argv,timeout in calls))

    def test_root_failure_keeps_20_rounds_and_no_settings(self):
        clock = Clock(); calls = []
        def invoke(argv, timeout):
            calls.append(argv)
            return result('1' if argv[-1] == 'sys.boot_completed' else '2000')
        with self.assertRaisesRegex(boot.BootFailure, 'not root-ready'):
            boot.ready(['adb'], invoke=invoke, clock=clock.now, sleep=clock.sleep)
        self.assertEqual(sum(argv[-2:] == ['id','-u'] for argv in calls),20)
        self.assertEqual(clock.value,10);self.assertEqual(calls.count(['adb','root']),1)
        self.assertFalse(any('settings' in argv for argv in calls))

    def test_observe_only_ready_never_roots_or_changes_settings(self):
        calls=[]
        def invoke(argv, timeout):
            calls.append(argv);return result('1' if argv[-1]=='sys.boot_completed' else '0')
        with contextlib.redirect_stdout(io.StringIO()):boot.ready(['adb'],True,invoke=invoke)
        self.assertEqual(calls,[['adb','shell','getprop','sys.boot_completed'],['adb','shell','id','-u']])

    def test_success_retains_touch_verification_and_only_matching_overlay_changes(self):
        calls=[]
        def invoke(argv,timeout):
            calls.append(argv)
            if argv[-1]=='-u':return result('0')
            if argv[-2:]==['overlay','list']:return result('[x] com.android.internal.emulation.pixel_7\n[x] unrelated.application\n')
            return result('1')
        with contextlib.redirect_stdout(io.StringIO()):boot.ready(['adb'],invoke=invoke)
        self.assertIn(['adb','shell','settings','put','system','show_touches','1'],calls)
        self.assertIn(['adb','shell','cmd','overlay','disable','--user','0','com.android.internal.emulation.pixel_7'],calls)
        self.assertFalse(any('unrelated.application' in argv for argv in calls))

    def test_verified_child_generation_exit_fails_without_restart_or_settings(self):
        clock=Clock(); children=iter([{'verified':True,'identity':(42,'10')},{'verified':True,'identity':None}]);calls=[]
        def invoke(argv,timeout):calls.append(argv);return result(code=1)
        with self.assertRaises(boot.BootFailure) as caught:
            boot.ready(['adb'],invoke=invoke,clock=clock.now,sleep=clock.sleep,child=lambda:next(children))
        self.assertEqual(caught.exception.phase,'owned_child_exited');self.assertEqual(clock.value,1)
        self.assertEqual(len(calls),2)

    def test_unknown_or_unsupported_owner_never_implies_child_exit(self):
        clock=Clock()
        with self.assertRaises(boot.BootFailure) as caught:
            boot.ready(['adb'],invoke=lambda *_:result('0'),clock=clock.now,sleep=clock.sleep,child=lambda:{'verified':False})
        self.assertEqual(caught.exception.phase,'boot_failed');self.assertEqual(clock.value,120)

    def test_cli_emits_diagnostic_before_original_failure_and_diagnostic_failure_cannot_hide_it(self):
        failure=boot.BootFailure('boot_failed','original failure');output=io.StringIO()
        with patch.dict(os.environ,{'AI_PHONE_PROJECT_ROOT':'/owned','AI_PHONE_SERIAL':'private-serial'}),patch.object(boot,'ready',side_effect=failure),patch.object(diag,'resources',return_value={'platform':'unsupported'}),patch.object(diag,'collect',return_value={'format':1,'phase':'boot_failed','adb':{'state':'offline'}}),contextlib.redirect_stdout(output):
            with self.assertRaisesRegex(SystemExit,'original failure'):boot.main(['/owned/sdk/platform-tools/adb'])
        value=json.loads(output.getvalue().split(' ',1)[1]);self.assertEqual(value['adb']['state'],'offline');self.assertNotIn('private-serial',output.getvalue())
        output=io.StringIO()
        with patch.dict(os.environ,{'AI_PHONE_PROJECT_ROOT':'/owned'}),patch.object(boot,'ready',side_effect=failure),patch.object(diag,'resources',return_value={}),patch.object(diag,'collect',side_effect=OSError('private')),contextlib.redirect_stdout(output):
            with self.assertRaisesRegex(SystemExit,'original failure'):boot.main(['/owned/adb'])
        self.assertEqual(json.loads(output.getvalue().split(' ',1)[1]),{'format':1,'diagnostic':'unavailable'})

    def test_missing_adb_fails_immediately_instead_of_hiding_launch_error_for_120_seconds(self):
        clock=Clock()
        with self.assertRaises(boot.BootFailure) as caught:
            boot.ready(['missing'],invoke=lambda *_:(_ for _ in ()).throw(FileNotFoundError()),clock=clock.now,sleep=clock.sleep)
        self.assertEqual(caught.exception.phase,'adb_unavailable');self.assertEqual(clock.value,0)


class DiagnosticTests(unittest.TestCase):
    def test_probe_fields_are_allowlisted_bounded_and_never_echo_guest_or_stderr(self):
        clock=Clock();calls=[]
        def invoke(argv,timeout):
            calls.append((argv,timeout));clock.value+=timeout
            if argv[-1]=='get-state':return result(code=1,stderr='private-token serial ABC error: device offline')
            return result('private-guest-message')
        output=diag.boot_probes(['adb','-s','private-serial'],3,invoke,clock.now)
        self.assertEqual(output['state'],'offline');self.assertEqual(set(output['properties']),set(diag.BOOT_PROPERTIES))
        self.assertTrue(all(value=='unknown' for value in output['properties'].values()))
        self.assertEqual(len(calls),3);self.assertLessEqual(clock.value,3)
        self.assertNotIn('private',json.dumps(output))

    def test_actual_acceleration_result_and_non_linux_boundary(self):
        calls=[]
        def invoke(argv,timeout):calls.append(argv);return result('private output',17)
        self.assertEqual(diag.acceleration(['/sdk/platform-tools/adb'],8,invoke,lambda:0,'Linux'),{'status':'unavailable','return_code':17})
        self.assertEqual(calls,[['/sdk/emulator/emulator','-accel-check']])
        self.assertEqual(diag.acceleration(['adb'],8,invoke,lambda:0,'Darwin')['status'],'unsupported');self.assertEqual(len(calls),1)
        self.assertEqual(diag.resources(system='Darwin'),{'platform':'unsupported'})
        self.assertEqual(diag.owned_process(Path('/unused'),system='Darwin'),{'verified':False,'platform':'unsupported'})

    def test_historical_exit_is_numeric_only_requires_owner_and_rejects_links(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);logs=root/'data/services';logs.mkdir(parents=True);path=logs/'emulator.log'
            path.write_bytes(b'private-token\n'+b'x'*10000+b'\nOwned child exited: emulator -9\nother private content\n')
            self.assertEqual(diag.last_owned_exit(root,{'verified':True}),-9)
            self.assertIsNone(diag.last_owned_exit(root,{'verified':False}))
            path.unlink();foreign=root/'foreign';foreign.write_text('Owned child exited: emulator 8\n');path.symlink_to(foreign)
            self.assertIsNone(diag.last_owned_exit(root,{'verified':True}));self.assertEqual(foreign.read_text(),'Owned child exited: emulator 8\n')

    def test_installed_data_alias_is_valid_but_foreign_alias_does_not_authorize_log_reads(self):
        with tempfile.TemporaryDirectory() as folder:
            home=Path(folder);root=home/'releases'/('a'*64);root.mkdir(parents=True)
            data=home/'user/data';(data/'services').mkdir(parents=True)
            (data/'services/emulator.log').write_text('Owned child exited: emulator 17\n')
            (root/'data').symlink_to(data)
            self.assertEqual(diag.last_owned_exit(root,{'verified':True}),17)
            (root/'data').unlink();foreign=home/'foreign';(foreign/'services').mkdir(parents=True)
            (foreign/'services/emulator.log').write_text('Owned child exited: emulator 9\n');(root/'data').symlink_to(foreign)
            self.assertIsNone(diag.last_owned_exit(root,{'verified':True}))

    def test_proc_children_stay_under_verified_parent_and_exclude_unrelated_process(self):
        with tempfile.TemporaryDirectory() as folder:
            proc=Path(folder);parent=proc/'100/task/100';parent.mkdir(parents=True);(parent/'children').write_text('101 999')
            for pid,ppid in [(101,100),(999,1)]:
                p=proc/str(pid);p.mkdir();(p/'status').write_text(f'PPid:\t{ppid}\n');(p/'stat').write_text(f'{pid} (private command) S '+' '.join(['0']*18+['123']))
            owned=diag.owned_process(Path('/owned'),proc=proc,system='Linux',query=lambda *_:{'pid':100})
            self.assertEqual(owned['identity'],(101,'123'))
            (parent/'children').write_text('999')
            self.assertIsNone(diag.owned_process(Path('/owned'),proc=proc,system='Linux',query=lambda *_:{'pid':100})['identity'])

    def test_optional_owner_probe_cannot_create_state_or_read_an_alias(self):
        import linux_services
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with patch.object(linux_services, 'process_start') as identity:
                self.assertFalse(diag.owned_process(root, system='Linux')['verified'])
                self.assertFalse((root/'data').exists())
                data = root/'data/services'
                data.mkdir(parents=True)
                secret = root/'private'
                secret.write_text('{}')
                (data/'emulator.json').symlink_to(secret)
                self.assertFalse(diag.owned_process(root, system='Linux')['verified'])
                identity.assert_not_called()

    def test_regular_reads_reject_fifo_and_over_budget_without_mutating_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            fifo = root/'fifo'
            os.mkfifo(fifo)
            with self.assertRaises(ValueError):
                diag.bounded(fifo)
            oversized = root/'large'
            oversized.write_bytes(b'x'*8193)
            with self.assertRaises(ValueError):
                diag.bounded(oversized)
            self.assertEqual(oversized.stat().st_size, 8193)

    def test_memory_fixture_is_numeric_and_oom_delta_never_becomes_causal_claim(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);proc=root/'proc';cg=root/'cgroup';(proc/'self').mkdir(parents=True);(cg/'job').mkdir(parents=True)
            (proc/'meminfo').write_text('MemTotal: 16000 kB\nMemAvailable: 9000 kB\nSwapFree: 4 kB\nSecret: 123 token\n')
            (proc/'self/cgroup').write_text('0::/job\n')
            for name,text in {'memory.current':'2048','memory.max':'max','cpu.max':'200000 100000','memory.events':'oom 3\noom_kill 2\nsecret 999'}.items():(cg/'job'/name).write_text(text)
            with patch.object(os,'sched_getaffinity',return_value={0,1},create=True):value=diag.resources(proc,cg,system='Linux')
            self.assertEqual(value['memory_kib'],{'MemTotal':16000,'MemAvailable':9000,'SwapFree':4});self.assertEqual(value['cgroup']['events'],{'oom':3,'oom_kill':2})
            self.assertEqual(diag.event_delta({'cgroup':{'events':{'oom_kill':1}}},value),{'oom_kill':1})
            self.assertNotIn('secret',json.dumps(value).lower())

    def test_real_owned_probe_caps_capture_and_terminates_only_its_child(self):
        output=diag.bounded_probe([sys.executable,'-c','import sys;sys.stdout.write("x"*20000);sys.stderr.write("y"*20000)'],2)
        self.assertEqual(output.returncode,0);self.assertEqual(len(output.stdout),4096);self.assertEqual(len(output.stderr),4096)
        self.assertIsNone(diag.bounded_probe([sys.executable,'-c','import time;time.sleep(10)'],.05))

    def test_parent_exit_with_inherited_pipe_reclaims_only_owned_descendants(self):
        with tempfile.TemporaryDirectory() as folder:
            lock_path = Path(folder)/'held.lock'
            marker = Path(folder)/'child.pid'
            code = """import fcntl,os,sys,time
from pathlib import Path
lock,marker=map(Path,sys.argv[1:])
if os.fork():
 while not marker.exists():time.sleep(.001)
 os._exit(17)
with lock.open('w') as stream:
 fcntl.flock(stream,fcntl.LOCK_EX)
 marker.write_text(str(os.getpid()))
 time.sleep(30)
"""
            unrelated = subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])
            try:
                output = diag.bounded_probe([sys.executable,'-c',code,str(lock_path),str(marker)],.3)
                self.assertIsNone(output)
                self.assertTrue(marker.exists(), 'owned descendant must actually acquire its lock')
                released = False
                with lock_path.open('r+') as stream:
                    deadline = time.monotonic()+.5
                    while time.monotonic()<deadline:
                        try:
                            fcntl.flock(stream,fcntl.LOCK_EX|fcntl.LOCK_NB)
                            released = True
                            break
                        except BlockingIOError:
                            time.sleep(.005)
                self.assertTrue(released, 'timed-out probe leaked its descendant after direct parent exited')
                self.assertIsNone(unrelated.poll(), 'unrelated process must remain alive')
            finally:
                # Fixture owns this exact child and unrelated sentinel; no host enumeration.
                if marker.exists():
                    try:os.kill(int(marker.read_text()),signal.SIGKILL)
                    except ProcessLookupError:pass
                unrelated.kill();unrelated.wait(timeout=2)

    def test_complete_report_no_private_values_and_no_oom_inference(self):
        state={'observations':120,'last_boot':{'return_code':1,'timed_out_or_unavailable':False,'boot_completed':'unknown'}}
        after={'platform':'linux','cgroup':{'events':{'oom_kill':2}}};before={'platform':'linux','cgroup':{'events':{'oom_kill':1}}}
        with patch.object(diag,'resources',return_value=after),patch.object(diag,'process_report',return_value={'verified':False,'current_exit_code':None}),patch.object(diag,'applied_configuration',return_value={'phone_cpus':2}),patch.object(diag.platform,'system',return_value='Linux'):
            report=diag.collect(Path('/private/path'),['/private/sdk/platform-tools/adb','-s','SECRET'], 'boot_failed',state,before,0,invoke=lambda *_:result('arbitrary private text',1,'token'),clock=lambda:120)
        self.assertFalse(report['oom_cause_confirmed']);self.assertEqual(report['cgroup_oom_delta'],{'oom_kill':1})
        encoded=json.dumps(report);self.assertNotIn('private',encoded);self.assertNotIn('SECRET',encoded);self.assertNotIn('token',encoded);self.assertLess(len(encoded),16384)


if __name__=='__main__':unittest.main()
