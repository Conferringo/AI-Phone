"""Small executable fake-ADB recovery tests; no device or real package is used."""
import ast
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import types
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
IME = 'dev.aiphone.input/.UnicodeIme'
ATTENTION = 'dev.aiphone.input/.PhoneAttention'
NOTIFICATION = 'dev.aiphone.input/.PhoneNotifications'
FAKE = r'''import json,os,sys
from pathlib import Path
p=Path(os.environ['INPUT_FIXTURE_STATE']);s=json.loads(p.read_text());args=sys.argv[1:]
s['calls'].append(args);a=args[4:]
def save():p.write_text(json.dumps(s))
def fail():
 save();print('private-provider-text',file=sys.stderr);raise SystemExit(7)
save()
if args[:4]!=['-P','15037','-s','emulator-fixture']:fail()
if s.get('offline'):fail()
matched=bool(s.get('fail_at') and a[:len(s['fail_at'])]==s['fail_at'])
if matched:
 s.pop('fail_at');save()
 if not s.get('after_apply'):fail()
if a==['shell','pm','list','packages','--user','0','dev.aiphone.input']:
 if s.get('package'):print('package:dev.aiphone.input')
elif a==['shell','am','get-current-user']:print(s.get('user','0'))
elif a and a[0]=='install':
 pending=p.parent/'data/android/input-setup.pending.json'
 s['pending_at_install']=json.loads(pending.read_text()) if pending.exists() else None
 if s.get('install_visible',True):s['package']=True
 print('Success')
elif a==['shell','ime','list','-a','-s','--user','0']:
 s['lists']=s.get('lists',0)+1
 if s['lists']>=s.get('ready_after',1):print(s.get('component','dev.aiphone.input/.UnicodeIme'))
elif a==['shell','ime','enable','--user','0','dev.aiphone.input/.UnicodeIme']:
 old=s['settings']['enabled_input_methods'];s['settings']['enabled_input_methods']=old+':dev.aiphone.input/.UnicodeIme' if old else 'dev.aiphone.input/.UnicodeIme'
elif a==['shell','ime','set','--user','0','dev.aiphone.input/.UnicodeIme']:
 s['settings']['default_input_method']='dev.aiphone.input/.UnicodeIme'
elif a[:3]==['shell','settings','--user'] and a[3]=='0':
 if a[4]=='get':print(s['settings'].get(a[6],'null'))
 elif a[4]=='put':s['settings'][a[6]]=a[7]
 else:fail()
elif a==['shell','cmd','notification','allow_listener','dev.aiphone.input/.PhoneNotifications','0']:
 component='dev.aiphone.input/dev.aiphone.input.PhoneNotifications' if s.get('expanded_components') else 'dev.aiphone.input/.PhoneNotifications'
 old=s['settings']['enabled_notification_listeners'];s['settings']['enabled_notification_listeners']=old+':'+component if old else component
else:fail()
save()
if matched:fail()
'''


class InputRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='input-recovery-', dir=HERE.parent)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.scripts = self.root/'scripts';self.scripts.mkdir()
        sdk=self.root/'sdk';(sdk/'platform-tools').mkdir(parents=True)
        adb=sdk/'platform-tools/adb'
        adb.write_text('#!'+str(Path(sys.executable).resolve())+'\n'+FAKE);adb.chmod(0o700)
        self.state=self.root/'fake-state.json'
        self.pending=self.root/'data/android/input-setup.pending.json'
        self.apk=self.root/'fixture.apk';self.apk.write_bytes(b'fixture')
        self.env={**os.environ,'PYTHONDONTWRITEBYTECODE':'1','AI_PHONE_SERIAL':'emulator-fixture','INPUT_FIXTURE_STATE':str(self.state)}
        for name in ('android_input.py','android-install-input.sh','private_files.py','installer_lock.py'):
            shutil.copyfile(HERE/name,self.scripts/name)
        (self.scripts/'android-env.sh').write_text('AI_PHONE_PROJECT_ROOT='+shlex.quote(str(self.root))+'\nANDROID_HOME='+shlex.quote(str(sdk))+'\n')
        (self.scripts/'prebuilt_phone.py').write_text('from pathlib import Path\np=Path('+repr(str(self.root/'resolved'))+')\np.write_text("called")\nprint('+repr(str(self.apk))+')\n')
        self.reset()

    def reset(self, **values):
        self.pending.unlink(missing_ok=True)
        state={'calls':[],'package':False,'settings':{'enabled_input_methods':'other/.Keyboard;7','default_input_method':'other/.Keyboard','show_ime_with_hard_keyboard':'0','enabled_notification_listeners':'other/.Listener','enabled_accessibility_services':'other/.Reader','accessibility_enabled':'0'},**values}
        self.state.write_text(json.dumps(state))

    def state_value(self):return json.loads(self.state.read_text())
    def calls(self):return [a[4:] for a in self.state_value()['calls']]
    def installs(self):return [a for a in self.calls() if a[0]=='install']
    def run_owner(self, **env):
        result=subprocess.run(['sh',str(self.scripts/'android-install-input.sh')],env={**self.env,**env},capture_output=True,text=True,timeout=12)
        self.assertNotIn('private-provider-text',result.stderr)
        return result
    def succeeded(self,result):self.assertEqual(result.returncode,0,result.stderr)
    def failed(self,result):self.assertNotEqual(result.returncode,0)
    def journal(self, **values):
        self.pending.parent.mkdir(parents=True,exist_ok=True)
        self.pending.write_text(json.dumps({'format':1,'package':'dev.aiphone.input','serial':'emulator-fixture','user':'0','stage':'install_unknown',**values}));self.pending.chmod(0o600)

    def wait_for(self, marker, process):
        deadline=time.monotonic()+5
        while not marker.exists() and process.poll() is None and time.monotonic()<deadline:time.sleep(.01)
        self.assertTrue(marker.exists(),'fixture did not enter the transaction')

    def release_child(self, process, release):
        release.write_text('release')
        try:return process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill();process.communicate(timeout=5);raise

    def test_direct_cli_concurrent_setup_is_rejected_before_adb(self):
        entered=self.root/'entered';release=self.root/'release'
        resolver=self.scripts/'prebuilt_phone.py'
        resolver.write_text('''import os,time
from pathlib import Path
root=Path(__file__).resolve().parent.parent
if os.environ.get('INPUT_FIXTURE_HOLD'):
 (root/'entered').write_text('entered')
 deadline=time.monotonic()+8
 while not (root/'release').exists():
  if time.monotonic()>deadline:raise SystemExit('fixture release timed out')
  time.sleep(.01)
print(root/'fixture.apk')
''')
        first=subprocess.Popen(['sh',str(self.scripts/'android-install-input.sh')],env={**self.env,'INPUT_FIXTURE_HOLD':'1'},stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            self.wait_for(entered,first)
            before=self.calls()
            second=subprocess.run(['sh',str(self.scripts/'android-install-input.sh')],env=self.env,capture_output=True,text=True,timeout=2)
            conflict_calls=self.calls()
        finally:stdout,stderr=self.release_child(first,release)
        self.assertEqual(first.returncode,0,stderr)
        self.assertEqual(len(self.installs()),1,'concurrent CLI callers must never install twice')
        self.failed(second);self.assertIn('already in progress',second.stderr)
        self.assertEqual(conflict_calls,before,'a lock conflict must not invoke ADB')
        self.assertFalse(self.pending.exists())
        self.succeeded(self.run_owner());self.assertEqual(len(self.installs()),1)

    def test_lock_conflict_precedes_pending_read_and_has_no_adb_side_effect(self):
        self.journal();self.pending.write_text('{broken')
        before=self.pending.read_bytes();entered=self.root/'entered';release=self.root/'release'
        code='''import sys,time
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from installer_lock import acquire
root=Path(sys.argv[2])
with acquire(root/'data/android/input-setup.lock'):
 (root/'entered').write_text('entered')
 deadline=time.monotonic()+8
 while not (root/'release').exists():
  if time.monotonic()>deadline:raise SystemExit('fixture release timed out')
  time.sleep(.01)
'''
        holder=subprocess.Popen([sys.executable,'-B','-c',code,str(self.scripts),str(self.root)],env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            self.wait_for(entered,holder)
            result=subprocess.run(['sh',str(self.scripts/'android-install-input.sh')],env=self.env,capture_output=True,text=True,timeout=2)
        finally:stdout,stderr=self.release_child(holder,release)
        self.assertEqual(holder.returncode,0,stderr)
        self.failed(result);self.assertIn('already in progress',result.stderr)
        self.assertEqual(self.calls(),[]);self.assertEqual(self.pending.read_bytes(),before)
        result=self.run_owner();self.failed(result);self.assertIn('pending owner is invalid',result.stderr)
        self.assertEqual(self.calls(),[])

    def test_registration_failure_then_reopen_finishes_without_reinstall(self):
        self.reset(fail_at=['shell','ime','list'])
        self.failed(self.run_owner());self.assertTrue(self.pending.exists())
        self.succeeded(self.run_owner());self.assertEqual(len(self.installs()),1)
        self.assertEqual(self.installs()[0],['install','--no-incremental','-r','--user','0',str(self.apk)])
        self.assertFalse(self.pending.exists())
        self.assertEqual(self.state_value()['settings']['default_input_method'],IME)

    def test_legacy_install_has_no_pending_and_preserves_every_preference(self):
        self.reset(package=True);before=self.state_value()['settings']
        self.succeeded(self.run_owner())
        self.assertEqual(self.state_value()['settings'],before);self.assertFalse(self.installs())
        self.assertEqual(self.calls(),[['shell','pm','list','packages','--user','0','dev.aiphone.input']])
        self.assertFalse((self.root/'resolved').exists());self.assertFalse(self.pending.exists())

    def test_unknown_write_is_observed_before_any_repeat(self):
        writes=[['shell','ime','enable'],['shell','ime','set'],['shell','settings','--user','0','put','secure','show_ime_with_hard_keyboard'],['shell','cmd','notification'],['shell','settings','--user','0','put','secure','enabled_accessibility_services'],['shell','settings','--user','0','put','secure','accessibility_enabled']]
        for write in writes:
            with self.subTest(write=write):
                self.reset(fail_at=write,after_apply=True)
                self.failed(self.run_owner());self.assertTrue(self.pending.exists())
                cut=len(self.calls());self.succeeded(self.run_owner())
                self.assertEqual(len(self.installs()),1)
                self.assertEqual(sum(a[:len(write)]==write for a in self.calls()),1)
                self.assertEqual(self.calls()[cut],['shell','pm','list','packages','--user','0','dev.aiphone.input'])
                self.assertFalse(self.pending.exists())

    def test_unknown_install_observes_present_package_without_replaying(self):
        self.reset(fail_at=['install'],after_apply=True)
        self.succeeded(self.run_owner());self.assertEqual(len(self.installs()),1)
        self.assertFalse(self.pending.exists())

    def test_unknown_install_with_absent_package_never_replays(self):
        self.reset(install_visible=False,fail_at=['install'])
        first=self.run_owner();self.failed(first);self.assertIn('unknown',first.stderr)
        self.assertEqual(json.loads(self.pending.read_text())['stage'],'install_unknown')
        second=self.run_owner();self.failed(second);self.assertIn('unknown',second.stderr)
        self.assertEqual(len(self.installs()),1)

    def test_disconnect_before_install_cannot_create_pending_or_completion(self):
        self.reset(offline=True);self.failed(self.run_owner())
        self.assertFalse(self.pending.exists());self.assertFalse(self.installs())

    def test_wrong_serial_corrupt_and_foreign_journals_fail_before_adb(self):
        for change in ({'serial':'other'},{'package':'foreign.app'},{'format':True},{'stage':'complete'}):
            with self.subTest(change=change):
                self.reset();self.journal(**change);before=self.pending.read_bytes()
                self.failed(self.run_owner());self.assertEqual(self.calls(),[]);self.assertEqual(self.pending.read_bytes(),before)
        self.reset();self.pending.parent.mkdir(parents=True,exist_ok=True);self.pending.write_text('{broken')
        self.failed(self.run_owner());self.assertEqual(self.calls(),[])

    def test_wrong_user_refuses_install_without_pending(self):
        self.reset(user='10');self.failed(self.run_owner())
        self.assertFalse(self.installs());self.assertFalse(self.pending.exists())

    def test_delayed_exact_registration_and_existing_services_are_preserved(self):
        self.reset(ready_after=2);self.succeeded(self.run_owner())
        calls=self.calls();lists=[i for i,a in enumerate(calls) if a[:3]==['shell','ime','list']]
        first_write=next(i for i,a in enumerate(calls) if a[:3]==['shell','ime','enable'])
        self.assertEqual(len(lists),2);self.assertLess(lists[-1],first_write)
        state=self.state_value()['settings']
        self.assertEqual(state['enabled_accessibility_services'],'other/.Reader:'+ATTENTION)
        self.assertEqual(state['enabled_notification_listeners'],'other/.Listener:'+NOTIFICATION)
        self.assertEqual(state['enabled_input_methods'],'other/.Keyboard;7:'+IME)

    def test_pending_is_private_before_single_install(self):
        self.reset(fail_at=['shell','ime','list']);self.failed(self.run_owner())
        self.assertEqual(self.pending.stat().st_mode&0o777,0o600)
        self.assertEqual(json.loads(self.pending.read_text())['stage'],'configure')
        self.assertEqual(self.state_value()['pending_at_install'],{'format':1,'package':'dev.aiphone.input','serial':'emulator-fixture','user':'0','stage':'install_unknown'})

    def test_android_expanded_components_are_observed_without_duplicate_writes(self):
        self.reset(expanded_components=True,fail_at=['shell','cmd','notification'],after_apply=True)
        self.failed(self.run_owner());self.succeeded(self.run_owner())
        self.assertEqual(sum(a[:3]==['shell','cmd','notification'] for a in self.calls()),1)

    def test_malformed_successful_settings_read_does_not_authorize_a_write(self):
        self.reset(package=True);self.journal(stage='configure')
        value=self.state_value();value['settings']['default_input_method']='Error: provider unavailable';self.state.write_text(json.dumps(value))
        self.failed(self.run_owner())
        self.assertFalse(any(a[:3]==['shell','ime','set'] for a in self.calls()))


def extracted(name,function,environment):
    tree=ast.parse((HERE/name).read_text())
    node=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name==function)
    exec(compile(ast.Module(body=[node],type_ignores=[]),name,'exec'),environment)
    return environment[function]


class CallerDelegationTests(unittest.TestCase):
    def test_linux_delegates_input_once_without_its_own_package_decision(self):
        run=[];queried=[]
        def query(argv,**_):queried.append(argv);return types.SimpleNamespace(returncode=0,stdout='package:fixture')
        function=extracted('linux_host.py','start_guest',{'subprocess':types.SimpleNamespace(run=query)})
        function(Path('/fixture'),{},run.append,lambda *_:False)
        self.assertEqual(sum(str(a[-1]).endswith('/android-install-input.sh') for a in run),1)
        self.assertFalse(any('dev.aiphone.input' in a for a in queried))
        self.assertEqual({a[-1] for a in queried},{'dev.aiphone.home','com.github.metacubex.clash.meta'})

    def test_mac_delegates_input_once_and_keeps_legacy_apps_untouched(self):
        run=[];queried=[];services=[('dev.aiphone.emulator.p2','android-start.sh')]
        env={'Path':Path,'os':os,'SERVICES':services,'owned_jobs':lambda *_:{services[0][0]:True},'runtime_env':lambda *_:{},'subprocess':subprocess}
        function=extracted('service_start.py','_start_missing',env)
        with patch.dict(sys.modules,{'managed_engine':types.SimpleNamespace(ensure_running=lambda *_:None)}):
            function('/fixture',execute=run.append,package_installed=lambda p:queried.append(p) or True)
        self.assertEqual(run,['android-install-input.sh'])
        self.assertEqual(queried,['dev.aiphone.home','com.github.metacubex.clash.meta'])


if __name__=='__main__':unittest.main()
