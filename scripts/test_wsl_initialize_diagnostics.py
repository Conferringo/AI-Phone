"""Synthetic files and mocked processes only; never starts a host engine or device."""
import os
import unittest

if os.name!='posix':
    raise unittest.SkipTest('WSL initialization owner fixtures require POSIX')

from contextlib import ExitStack
from copy import deepcopy
import io
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
from unittest.mock import Mock,patch

import wsl_backend_entry as entry
import wsl_initialize_diagnostics as diag
import wsl_runtime_fixture as fixture
import wsl_runtime_worker as worker


def receipt():
    return {'format':1,'nonce':'a'*32,'phase':1,'status':'failed','stage':'initialize',
        'checks':{name:False for name in fixture.CHECKS},'frame':{},'windows_boot_verified':False,
        'phone_ready':False,'initialization':diag.initial()}


class NoHostProcesses(unittest.TestCase):
    def setUp(self):
        for name in ('run','Popen'):
            guard=patch.object(subprocess,name,side_effect=AssertionError('Real host process forbidden'))
            guard.start();self.addCleanup(guard.stop)


class EngineEvidenceTests(NoHostProcesses):
    def owned(self,root):
        folder=root/'data/services';folder.mkdir(parents=True)
        return folder/'wsl-engine.log'

    def test_only_fixed_flags_leave_bounded_current_phase_bytes(self):
        with tempfile.TemporaryDirectory() as temporary,patch('linux_services.unit_state',return_value={'pid':1}):
            root=Path(temporary);log=self.owned(root);log.write_bytes(b'iptables private-old-phase\n')
            before=diag.engine_cursor(root)
            with log.open('ab') as output:output.write(b'x'*(diag.LIMIT+1)+b'\xff permission denied secret-token /private/path\n')
            value=diag.engine_report(root,before)
            self.assertEqual(value['flags'],['permission_message']);self.assertEqual(value['bytes_read'],diag.LIMIT)
            self.assertTrue(value['truncated']);self.assertEqual(value['state'],'observed')
            self.assertNotIn('private',json.dumps(value));self.assertNotIn('secret',json.dumps(value))
            diag.checked_log(value)

    def test_every_signal_has_only_a_closed_code_and_unknown_text_is_unclassified(self):
        with tempfile.TemporaryDirectory() as temporary,patch('linux_services.unit_state',return_value={'pid':1}):
            root=Path(temporary);log=self.owned(root)
            for code,patterns in diag.SIGNALS.items():
                log.write_bytes(patterns[0]+b' token=private /home/user/secret')
                self.assertEqual(diag.engine_report(root,None)['flags'],[code])
            log.write_bytes(b'unknown failure token=private\xff')
            value=diag.engine_report(root,None)
            self.assertEqual(value['flags'],[]);self.assertEqual(value['state'],'observed')

    def test_exact_read_budget_empty_and_current_phase_offset(self):
        actual_reader=diag.regular_reader;reads=[]
        class BoundedReader:
            def __init__(self,path):self.stream=actual_reader(path)
            def __enter__(self):return self
            def __exit__(self,*_):self.stream.close()
            def fileno(self):return self.stream.fileno()
            def seek(self,*args):return self.stream.seek(*args)
            def read(self,size):
                if not 0<=size<=diag.LIMIT:raise AssertionError('Unbounded read')
                reads.append(size);return self.stream.read(size)
        with tempfile.TemporaryDirectory() as temporary,patch('linux_services.unit_state',return_value={'pid':1}):
            root=Path(temporary);log=self.owned(root)
            for size in (0,1,diag.LIMIT,diag.LIMIT+1):
                log.write_bytes(b'x'*size)
                with patch.object(diag,'regular_reader',side_effect=BoundedReader):value=diag.engine_report(root,None)
                self.assertEqual(value['bytes_read'],min(size,diag.LIMIT));self.assertEqual(value['truncated'],size>diag.LIMIT)
            before=diag.engine_cursor(root)
            self.assertEqual(diag.engine_report(root,before),diag.log_result('empty'))
        self.assertEqual(reads,[0,1,diag.LIMIT,diag.LIMIT])

    def test_owner_alias_nonregular_and_changed_generation_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary,patch('linux_services.unit_state',return_value={'pid':1}):
            root=Path(temporary);log=self.owned(root);foreign=root/'foreign';foreign.write_bytes(b'permission denied')
            log.symlink_to(foreign)
            self.assertIs(diag.engine_cursor(root),False);self.assertEqual(diag.engine_report(root,None)['state'],'unavailable')
            log.unlink();os.mkfifo(log)
            self.assertIs(diag.engine_cursor(root),False);self.assertEqual(diag.engine_report(root,None)['state'],'unavailable')
            log.unlink();log.mkdir();self.assertEqual(diag.engine_report(root,None)['state'],'unavailable');log.rmdir()
            log.write_bytes(b'old');before=diag.engine_cursor(root);log.rename(log.with_suffix('.previous'));log.write_bytes(b'new')
            self.assertEqual(diag.engine_report(root,before)['state'],'unverified')
            before=diag.engine_cursor(root);log.write_bytes(b'')
            self.assertEqual(diag.engine_report(root,before)['state'],'unverified')
            with patch.object(diag.os,'getuid',return_value=-1):
                self.assertIs(diag.engine_cursor(root),False);self.assertEqual(diag.engine_report(root,None)['state'],'unverified')
            log.unlink();log.with_suffix('.previous').unlink();log.parent.rmdir();log.parent.symlink_to(root)
            self.assertEqual(diag.engine_report(root,None)['state'],'unavailable')

    def test_existing_owner_and_data_alias_primitives_are_reused(self):
        with tempfile.TemporaryDirectory() as temporary,patch('linux_services.unit_state',return_value={'pid':1}):
            home=Path(temporary);root=home/'releases'/('a'*64);root.mkdir(parents=True)
            data=home/'user/data';(data/'services').mkdir(parents=True);(root/'data').symlink_to(data)
            (data/'services/wsl-engine.log').write_bytes(b'cgroup')
            self.assertEqual(diag.engine_report(root,None)['flags'],['cgroup_message'])
            (root/'data').unlink();(root/'data').symlink_to(home/'foreign')
            with patch.object(diag,'regular_reader') as read:
                self.assertEqual(diag.engine_report(root,None)['state'],'unavailable');read.assert_not_called()
        with patch('linux_services.unit_state',return_value=None),patch.object(diag,'regular_reader') as read:
            self.assertEqual(diag.engine_report(Path('/never-read'),None)['state'],'unverified');read.assert_not_called()

    def test_metadata_does_not_read_log_and_unreadable_or_unknown_errors_are_safe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);log=self.owned(root)
            with patch.object(diag,'regular_reader',side_effect=AssertionError('No pre-failure reads')):
                self.assertIsNone(diag.engine_cursor(root));log.write_bytes(b'private');self.assertIsInstance(diag.engine_cursor(root),tuple)
            with patch('linux_services.unit_state',return_value={'pid':1}):
                for failure in (PermissionError('private'),RuntimeError('private')):
                    with patch.object(diag,'regular_reader',side_effect=failure):self.assertEqual(diag.engine_report(root,None),diag.log_result('unavailable'))


class ErrorAndSchemaTests(NoHostProcesses):
    def test_exception_text_and_argv_never_enter_error_fields(self):
        errors=[(BlockingIOError('private'),'lock_busy'),(FileNotFoundError('/private'),'resource_missing'),
            (PermissionError('token=private'),'permission_denied'),(subprocess.TimeoutExpired(['private'],30,output=b'private'),'subprocess_timeout'),
            (subprocess.CalledProcessError(7,['private'],output='private',stderr='private'),'command_failed'),
            (ValueError('private'),'validation_failed'),(OSError('private'),'os_error'),(RuntimeError('private'),'unexpected')]
        for error,code in errors:
            value=diag.error_fields(error);self.assertEqual(value['error'],code)
            self.assertEqual(value['returncode'],7 if code=='command_failed' else None);self.assertNotIn('private',json.dumps(value))
        for bad in (True,0,'private',2**31,-2**31-1):
            self.assertIsNone(diag.error_fields(subprocess.CalledProcessError(bad,['private']))['returncode'])
        for accepted in (-2**31,-9,2**31-1):self.assertEqual(diag.error_fields(subprocess.CalledProcessError(accepted,[]))['returncode'],accepted)

    def test_closed_receipt_shapes_reject_injected_values_and_false_success(self):
        value=receipt();fixture.worker_result(value,'a'*32,1)
        changes=[('step',[]),('step','private'),('error',{}),('error','private'),('returncode',True),('returncode',1),
            ('engine',[]),('engine',{'state':'private'}),('private','secret')]
        for key,bad in changes:
            changed=deepcopy(value);changed['initialization'][key]=bad
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        for patch_value in ({'state':[]},{'state':'private'},{'flags':['private']},{'flags':[[]]},
                {'flags':['cgroup_message','cgroup_message']},{'bytes_read':True},{'bytes_read':diag.LIMIT+1},
                {'truncated':1},{'private':'secret'},{'state':'observed'},{'flags':['cgroup_message']}):
            changed=deepcopy(value);changed['initialization']['engine'].update(patch_value)
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        changed=deepcopy(value);changed.update(status='passed',stage='stopped',checks={name:True for name in fixture.CHECKS},frame={'width':1,'height':1,'sha256':'b'*64})
        with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        changed['initialization']['step']='complete';fixture.worker_result(changed,'a'*32,1)
        changed['initialization']['error']='unexpected'
        with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)

    def test_log_shape_and_phase_binding_are_strict(self):
        value=receipt();value['initialization'].update(step='engine_ready',error='command_failed',returncode=-9,
            engine={'state':'observed','flags':['cgroup_message'],'bytes_read':diag.LIMIT,'truncated':True})
        fixture.worker_result(value,'a'*32,1)
        for section,changes in [('initialization',{'step':'images'}),('initialization',{'error':'none'}),
                ('initialization',{'returncode':0}),('engine',{'bytes_read':1}),('engine',{'flags':list(diag.SIGNALS)})]:
            changed=deepcopy(value);target=changed['initialization'] if section=='initialization' else changed['initialization']['engine'];target.update(changes)
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        for stage in ('guard','nesting','readiness','desktop','stopped'):
            changed=deepcopy(value);changed['stage']=stage
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)


class WorkerFailureTests(NoHostProcesses):
    def test_engine_diagnostic_failure_preserves_the_exact_original_exception(self):
        for collector_error in (PermissionError('secret'),RuntimeError('secret')):
            failure=subprocess.CalledProcessError(17,['secret-command'],stderr='secret-password')
            def fail(_request,observer):
                entry.observe(observer,'engine_supervisor',Path('/owned-fixture'))
                entry.observe(observer,'engine_ready',Path('/owned-fixture'));raise failure
            with tempfile.TemporaryDirectory() as temporary,patch.object(worker,'STATE',Path(temporary)),patch.object(entry,'initialize',side_effect=fail),patch.object(diag,'engine_cursor',return_value=None),patch.object(diag,'engine_report',side_effect=collector_error):
                value=receipt()
                with self.assertRaises(subprocess.CalledProcessError) as caught:worker.initialize(value)
                self.assertIs(caught.exception,failure);self.assertEqual(value['initialization']['returncode'],17)
                self.assertEqual(value['initialization']['engine'],diag.log_result('unavailable'))
                self.assertNotIn('secret',json.dumps(value));fixture.worker_result(value,'a'*32,1)

    def test_non_engine_failure_and_success_never_read_an_engine_log(self):
        for failing in (True,False):
            failure=RuntimeError('private unknown failure')
            def initialize(_request,observer):
                entry.observe(observer,'images')
                if failing:raise failure
                return {'phone_ready':False,'windows_boot_verified':False}
            with tempfile.TemporaryDirectory() as temporary,patch.object(worker,'STATE',Path(temporary)),patch.object(entry,'initialize',side_effect=initialize),patch.object(entry,'staged',return_value=Path(temporary)),patch.object(diag,'engine_report') as report,patch.object(diag,'engine_cursor') as cursor:
                value=receipt()
                if failing:
                    with self.assertRaises(RuntimeError) as caught:worker.initialize(value)
                    self.assertIs(caught.exception,failure);self.assertEqual(value['initialization']['error'],'unexpected')
                else:
                    self.assertEqual(worker.initialize(value),Path(temporary));self.assertEqual(value['initialization']['step'],'complete')
                report.assert_not_called();cursor.assert_not_called()

    def test_failed_worker_keeps_checks_frame_flags_cleanup_contract_and_no_marker(self):
        def fail(_request,observer):entry.observe(observer,'tools');raise ValueError('secret-token /private')
        with tempfile.TemporaryDirectory() as temporary,patch.object(worker,'STATE',Path(temporary)),patch.object(worker,'fixture_identity',return_value='a'*32),patch.object(worker,'nested_cgroups'),patch.object(entry,'initialize',side_effect=fail),patch.object(worker,'shutdown') as stop:
            self.assertEqual(worker.main(),1)
            value=json.loads((Path(temporary)/'qualification-result.json').read_text())
            fixture.worker_result(value,'a'*32,1);self.assertEqual(value['initialization']['step'],'tools')
            self.assertFalse(any(value['checks'].values()));self.assertEqual(value['frame'],{})
            self.assertFalse(value['windows_boot_verified']);self.assertFalse(value['phone_ready']);stop.assert_called_once_with(None)
            self.assertFalse((Path(temporary)/'qualification-marker.json').exists());self.assertNotIn('secret',json.dumps(value))


class InitializeOrderingTests(NoHostProcesses):
    def run_initialize(self,root,fail_at=None,observer=None):
        failure=RuntimeError('private injected failure');effects=[];actual_path=Path
        def call(name,value=None):
            effects.append(name)
            if fail_at==name:raise failure
            return value
        def unit(_root,name,*_):return call('engine_supervisor' if name=='wsl-engine' else 'services_start')
        def command(argv,**_):
            step='engine_ready' if argv==['docker','info'] else 'database_start' if 'compose' in argv else 'database_prepare'
            call(step);return subprocess.CompletedProcess(argv,0)
        with ExitStack() as stack:
            stack.enter_context(patch.dict(os.environ,dict(os.environ)))
            stack.enter_context(patch.object(entry,'INSTALLATION',root/'installation'))
            stack.enter_context(patch.object(entry,'Path',side_effect=lambda value:Mock() if str(value)=='/run/aiphone' else actual_path(value)))
            stack.enter_context(patch.object(entry,'require_linux',side_effect=lambda:call('preflight')))
            stack.enter_context(patch.object(entry.shutil,'disk_usage',return_value=SimpleNamespace(free=8*1024**3)))
            stack.enter_context(patch.object(entry,'staged',side_effect=lambda _:call('source',root)))
            stack.enter_context(patch('installer_tools.select',side_effect=lambda *_:call('tools',{})))
            stack.enter_context(patch('client_host.prepare_profile',side_effect=lambda *_:call('profile')))
            stack.enter_context(patch('client_host.verify_container_ownership',side_effect=lambda *_:call('container_ownership',{})))
            stack.enter_context(patch('container_images.stage',return_value=None))
            stack.enter_context(patch('container_images.activate',side_effect=lambda *_:call('images',{})))
            stack.enter_context(patch('runtime_config.runtime_env',side_effect=lambda *_:call('environment',{})))
            stack.enter_context(patch('linux_host.private_token'))
            stack.enter_context(patch('linux_host.start_web_service',side_effect=lambda _r,_e,_run,start:start('web',['web'])))
            stack.enter_context(patch('linux_services.start_unit',side_effect=unit))
            stack.enter_context(patch('linux_services.unit_state',return_value=None))
            stack.enter_context(patch('prebuilt_services.launch_spec',return_value={'argv':['assistant'],'environment':{}}))
            stack.enter_context(patch('prebuilt_js.launch_spec',side_effect=lambda _r,name:{'argv':[name],'environment':{}}))
            stack.enter_context(patch.object(entry.subprocess,'run',side_effect=command))
            request=root/'request.json';request.write_text(json.dumps({'locale':'en','phone':{'cpus':2,'memory_mb':2048,'storage_gb':16},'desktop':{'cpus':2,'memory_mb':2048}}))
            try:result=entry.initialize(request,observer=observer)
            except RuntimeError as error:
                self.assertIs(error,failure);return effects,error
            return effects,result

    def test_twelve_substeps_precede_failures_without_continuing_later_operations(self):
        steps=['preflight','source','tools','profile','engine_supervisor','engine_ready','container_ownership','images','environment','database_start','database_prepare','services_start']
        for step in steps:
            with self.subTest(step=step),tempfile.TemporaryDirectory() as temporary:
                observed=[];effects,result=self.run_initialize(Path(temporary),step,lambda name,_root:observed.append(name))
                self.assertIsInstance(result,RuntimeError);self.assertEqual(observed[-1],step);self.assertEqual(effects[-1],step)
                self.assertEqual(observed,steps[:steps.index(step)+1])

    def test_no_observer_and_broken_observer_have_identical_production_effects(self):
        def broken(*_):raise RuntimeError('private observer failure')
        results=[]
        for observer in (None,broken):
            with tempfile.TemporaryDirectory() as temporary:results.append(self.run_initialize(Path(temporary),observer=observer))
        self.assertEqual(results[0],results[1]);self.assertTrue(results[0][1]['backend_started'])
        self.assertFalse(results[0][1]['phone_ready']);self.assertFalse(results[0][1]['windows_boot_verified'])

    def test_thirty_second_engine_deadline_is_unchanged(self):
        run=Mock(return_value=SimpleNamespace(returncode=1))
        with patch.object(entry.time,'monotonic',side_effect=[0,0,31]),patch.object(entry.time,'sleep') as sleep:
            with self.assertRaisesRegex(ValueError,'did not become ready'):entry.wait_engine(run)
        sleep.assert_called_once_with(.25);run.assert_called_once()


if __name__=='__main__':unittest.main()
