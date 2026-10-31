"""Mocked outer fixture only: current owned phase FD, no child processes or engine."""
import os
import unittest

if os.name!='posix':
    raise unittest.SkipTest('WSL initialization owner fixtures require POSIX')

from copy import deepcopy
import io
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
from unittest.mock import Mock,patch

import wsl_initialize_diagnostics as diag
import wsl_runtime_fixture as fixture


NONCE='a'*32
CONTAINER='b'*64


def receipt(phase=1):
    value={'format':1,'nonce':NONCE,'phase':phase,'status':'failed','stage':'initialize',
           'checks':{name:False for name in fixture.CHECKS},'frame':{},'windows_boot_verified':False,
           'phone_ready':False,'initialization':diag.initial()}
    value['initialization'].update(step='database_start',error='command_failed',returncode=7)
    return value


class NoChildProcesses(unittest.TestCase):
    def setUp(self):
        for name in ('run','Popen','call','check_call','check_output'):
            guard=patch.object(subprocess,name,side_effect=AssertionError('External child process forbidden'))
            guard.start();self.addCleanup(guard.stop)
        guard=patch.object(os,'system',side_effect=AssertionError('External child process forbidden'))
        guard.start();self.addCleanup(guard.stop)


class ClassificationTests(NoChildProcesses):
    def test_specific_error_signals_distinguish_configuration_image_health_and_task_failure(self):
        cases=[
            (b'Set POSTGRES_PASSWORD in .env', ['configuration_missing']),
            (b'Error response from daemon: No such image: fixed-fixture', ['image_unavailable']),
            (b'container ai-phone-postgres-1 is unhealthy', ['healthcheck_failed']),
            (b'failed to create task for container: OCI runtime create failed', ['container_runtime_failure']),
            (b'AppArmor profile could not be loaded', ['apparmor_message']),
            (b'error loading seccomp filter', ['seccomp_message']),
            (b'private unclassified compose failure', []),
        ]
        for body,expected in cases:
            with self.subTest(expected=expected):
                result=diag.database_report(io.BytesIO(b'secret=fixture /private\n'+body))
                self.assertEqual(result['flags'],expected)
                self.assertEqual(result['state'],'observed')
                serialized=json.dumps(result)
                self.assertNotIn('secret',serialized);self.assertNotIn('/private',serialized)

    def test_only_last_64k_are_read_and_absent_signal_remains_unknown(self):
        reads=[]
        class Bounded(io.BytesIO):
            def read(self,size=-1):
                self.assert_bound=0<=size<=diag.LIMIT
                if not self.assert_bound:raise AssertionError('Unbounded diagnostic read')
                reads.append(size);return super().read(size)
        body=b'no such image\n'+b'x'*(diag.LIMIT+2)
        result=diag.database_report(Bounded(body))
        self.assertEqual(reads,[diag.LIMIT]);self.assertEqual(result['bytes_read'],diag.LIMIT)
        self.assertTrue(result['truncated']);self.assertEqual(result['flags'],[])
        self.assertEqual(diag.database_report(io.BytesIO()),diag.log_result('empty'))

    def test_unreadable_output_and_unknown_exceptions_are_unavailable(self):
        for method in ('flush','seek','read'):
            for error in (OSError('private'),RuntimeError('private')):
                with self.subTest(method=method,error=type(error).__name__):
                    output=Mock(wraps=io.BytesIO(b'no such image'))
                    getattr(output,method).side_effect=error
                    self.assertEqual(diag.database_report(output),diag.log_result('unavailable'))

    def test_strict_schema_rejects_injected_values_and_wrong_failure_stage(self):
        value=receipt()['initialization'];value['database']=diag.database_report(io.BytesIO(b'no such image'))
        diag.checked(value)
        for change in ({'state':'private'},{'state':'unverified','flags':[],'bytes_read':0},
                       {'flags':['private']},{'bytes_read':True},{'bytes_read':diag.LIMIT+1},
                       {'truncated':1},{'raw':'secret'},{'flags':['image_unavailable','image_unavailable']}):
            bad=deepcopy(value);bad['database'].update(change)
            with self.assertRaises(ValueError):diag.checked(bad)
        for step,error in (('engine_ready','command_failed'),('complete','none'),('database_start','validation_failed')):
            bad=deepcopy(value);bad.update(step=step,error=error,returncode=None)
            with self.assertRaises(ValueError):diag.checked(bad)

    def test_passed_receipt_cannot_include_database_failure_evidence(self):
        value=receipt()['initialization']
        value['database']=diag.database_report(io.BytesIO(b'no such image'))
        diag.checked(value)
        with self.assertRaisesRegex(ValueError,'Incomplete initialization evidence'):
            diag.checked(value,passed=True)
        value.update(step='complete',error='none',returncode=None)
        with self.assertRaisesRegex(ValueError,'Unexpected database startup evidence'):
            diag.checked(value,passed=True)
        value['database']=diag.log_result()
        diag.checked(value,passed=True)


class OuterPhaseTests(NoChildProcesses):
    def phase(self,work,value,body=b'fixture unknown failure',returncode=1,after_write=None):
        process=Mock()
        def start(argv,**kwargs):
            self.assertEqual(argv,['docker','start','--attach',CONTAINER])
            self.assertEqual(set(kwargs),{'stdout','stderr','timeout'})
            self.assertEqual(kwargs['stderr'],subprocess.STDOUT);self.assertEqual(kwargs['timeout'],900)
            output=kwargs['stdout'];process.output=output;output.write(body)
            if after_write:after_write(output)
            return SimpleNamespace(returncode=returncode)
        query=Mock(side_effect=AssertionError('No extra Docker query'))
        with patch.object(fixture,'owned') as owner,patch.object(fixture,'read_phase',return_value=deepcopy(value)) as read,patch.object(subprocess,'run',side_effect=start) as run:
            result=fixture.run_phase(work,CONTAINER,NONCE,value['phase'],run=query)
        owner.assert_called_once_with('container',CONTAINER,NONCE,query)
        read.assert_called_once_with(work,CONTAINER,NONCE,value['phase'],query)
        self.assertEqual(run.call_count,1);query.assert_not_called()
        fixture.worker_result(result,NONCE,value['phase'])
        return result,process.output

    def test_current_owned_fd_is_reused_without_second_capture_or_path_reader(self):
        original=receipt();held=[];real_report=diag.database_report
        def report(output):
            self.assertIs(output,held[0]);self.assertFalse(output.closed)
            return real_report(output)
        with tempfile.TemporaryDirectory() as temporary,patch.object(tempfile,'TemporaryFile',side_effect=AssertionError('No duplicate capture')),patch.object(diag,'regular_reader',side_effect=AssertionError('No arbitrary log path')),patch.object(diag,'database_report',side_effect=report):
            result,output=self.phase(Path(temporary),original,b'no such image',after_write=lambda fd:held.append(fd))
            self.assertTrue(output.closed);self.assertEqual(result['initialization']['database']['flags'],['image_unavailable'])
            stripped=deepcopy(result);stripped['initialization']['database']=diag.log_result()
            self.assertEqual(stripped,original)

    def test_phase_two_never_reads_previous_phase_log(self):
        with tempfile.TemporaryDirectory() as temporary:
            work=Path(temporary);previous=work/'worker-phase-1.private.log'
            previous.write_bytes(b'private history no such image')
            result,_=self.phase(work,receipt(2),b'container ai-phone-redis-1 is unhealthy')
            self.assertEqual(result['initialization']['database']['flags'],['healthcheck_failed'])
            self.assertEqual(previous.read_bytes(),b'private history no such image')
            self.assertNotIn('history',json.dumps(result))

    def test_path_replacement_cannot_redirect_the_held_phase_fd(self):
        with tempfile.TemporaryDirectory() as temporary:
            work=Path(temporary);path=work/'worker-phase-1.private.log'
            def replace(output):
                output.flush();path.rename(work/'moved-owned-phase')
                foreign=work/'foreign';foreign.write_bytes(b'no such image private-foreign')
                path.symlink_to(foreign)
            result,_=self.phase(work,receipt(),b'container ai-phone-postgres-1 is unhealthy',after_write=replace)
            self.assertEqual(result['initialization']['database']['flags'],['healthcheck_failed'])
            self.assertNotIn('private-foreign',json.dumps(result))

    def test_existing_phase_log_and_owner_failure_are_rejected_before_process(self):
        with tempfile.TemporaryDirectory() as temporary,patch.object(fixture,'owned'),patch.object(subprocess,'run') as run,patch.object(diag,'database_report') as report:
            work=Path(temporary);(work/'worker-phase-1.private.log').write_bytes(b'old history')
            with self.assertRaises(FileExistsError):fixture.run_phase(work,CONTAINER,NONCE,1)
            run.assert_not_called();report.assert_not_called()
        with tempfile.TemporaryDirectory() as temporary,patch.object(fixture,'owned',side_effect=ValueError('unowned')),patch.object(subprocess,'run') as run:
            with self.assertRaisesRegex(ValueError,'unowned'):fixture.run_phase(Path(temporary),CONTAINER,NONCE,1)
            run.assert_not_called();self.assertEqual(list(Path(temporary).iterdir()),[])

    def test_only_failed_database_command_is_sampled(self):
        alternatives=[]
        healthy=receipt();healthy.update(status='passed',stage='stopped',checks={name:True for name in fixture.CHECKS},frame={'width':1,'height':1,'sha256':'c'*64});healthy['initialization']=diag.initial();healthy['initialization']['step']='complete'
        alternatives.append((healthy,0))
        for step,error in (('images','validation_failed'),('database_prepare','command_failed'),('engine_ready','command_failed'),('database_start','validation_failed')):
            value=receipt();value['initialization'].update(step=step,error=error,returncode=7 if error=='command_failed' else None)
            alternatives.append((value,1))
        alternatives.append((receipt(),0))
        for value,code in alternatives:
            with self.subTest(step=value['initialization']['step'],returncode=code),tempfile.TemporaryDirectory() as temporary,patch.object(diag,'database_report') as report:
                result,_=self.phase(Path(temporary),value,b'no such image',returncode=code)
                self.assertEqual(result['initialization']['database'],diag.log_result());report.assert_not_called()

    def test_classifier_fault_or_malformed_result_keeps_original_failure_and_rc(self):
        bad=[RuntimeError('private'),OSError('private'),{'raw':'private'},
             {'state':'unverified','flags':[],'bytes_read':0,'truncated':False}]
        for fault in bad:
            hook=patch.object(diag,'database_report',side_effect=fault) if isinstance(fault,Exception) else patch.object(diag,'database_report',return_value=fault)
            with tempfile.TemporaryDirectory() as temporary,hook:
                original=receipt();result,_=self.phase(Path(temporary),original,returncode=83)
                self.assertEqual(result['initialization']['returncode'],7)
                self.assertEqual(result['initialization']['error'],'command_failed')
                self.assertEqual(result['initialization']['database'],diag.log_result('unavailable'))
                self.assertEqual(result['status'],'failed');self.assertFalse(any(result['checks'].values()))
                self.assertEqual(result['frame'],{});self.assertFalse(result['phone_ready']);self.assertFalse(result['windows_boot_verified'])

    def test_original_read_phase_exception_is_not_replaced_or_classified(self):
        failure=subprocess.CalledProcessError(23,['fixed-fixture-command'])
        with tempfile.TemporaryDirectory() as temporary,patch.object(fixture,'owned'),patch.object(subprocess,'run',return_value=SimpleNamespace(returncode=1)),patch.object(fixture,'read_phase',side_effect=failure),patch.object(diag,'database_report') as report:
            with self.assertRaises(subprocess.CalledProcessError) as caught:fixture.run_phase(Path(temporary),CONTAINER,NONCE,1)
            self.assertIs(caught.exception,failure);self.assertEqual(caught.exception.returncode,23);report.assert_not_called()

    def test_original_start_exception_is_not_replaced_or_classified(self):
        failure=subprocess.CalledProcessError(29,['fixed-fixture-command'])
        with tempfile.TemporaryDirectory() as temporary,patch.object(fixture,'owned'),patch.object(subprocess,'run',side_effect=failure),patch.object(fixture,'read_phase') as read,patch.object(diag,'database_report') as report:
            with self.assertRaises(subprocess.CalledProcessError) as caught:fixture.run_phase(Path(temporary),CONTAINER,NONCE,1)
            self.assertIs(caught.exception,failure);self.assertEqual(caught.exception.returncode,29)
            read.assert_not_called();report.assert_not_called()

    def test_existing_timeout_path_stays_failed_without_inventing_command_evidence(self):
        value=receipt();value['initialization'].update(error='none',returncode=None)
        with tempfile.TemporaryDirectory() as temporary,patch.object(fixture,'owned'),patch.object(subprocess,'run',side_effect=subprocess.TimeoutExpired(['fixed-fixture'],900)),patch.object(fixture,'read_phase',return_value=value),patch.object(diag,'database_report') as report:
            result=fixture.run_phase(Path(temporary),CONTAINER,NONCE,1)
            self.assertEqual(result['status'],'failed');self.assertEqual(result['initialization']['database'],diag.log_result())
            self.assertIsNone(result['initialization']['returncode']);report.assert_not_called()


if __name__=='__main__':unittest.main()
