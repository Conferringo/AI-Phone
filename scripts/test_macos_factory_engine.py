"""Hosted factory ownership, software-engine argv and complete-build handoff."""
from contextlib import ExitStack
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock,patch

import macos_factory_engine as engine
from macos_factory_qemu import software_arguments


class MacFactoryEngineTests(unittest.TestCase):
    def test_software_accelerator_preserves_queries_and_other_device_arguments(self):
        self.assertEqual(software_arguments(['-M','none','-accel','help']),['-M','none','-accel','help'])
        self.assertEqual(software_arguments(['--version']),['--version'])
        self.assertEqual(software_arguments(['-machine','virt,accel=hvf,highmem=on','-cpu','max','-accel','hvf']),
            ['-machine','virt,accel=tcg,highmem=on','-cpu','max','-accel','tcg,thread=multi'])

    @unittest.skipUnless(os.name=='posix','Hosted Mac factory uses POSIX executables')
    def test_real_qemu_adapter_reexecutes_with_software_args_and_preserves_spaces(self):
        with tempfile.TemporaryDirectory(prefix='factory adapter ') as temp:
            folder=Path(temp);fake=folder/'fake qemu';program=folder/'record.py'
            program.write_text('import json,sys;print(json.dumps(sys.argv[1:]))')
            fake.write_text('#!/bin/sh\nexec '+shlex.quote(sys.executable)+' '+shlex.quote(str(program))+' "$@"\n');fake.chmod(0o755)
            adapter=Path(__file__).with_name('macos_factory_qemu.py')
            result=subprocess.run([sys.executable,adapter,'-machine','virt,accel=hvf','-drive','file=/tmp/with spaces'],
                env={**os.environ,'AI_PHONE_FACTORY_QEMU':str(fake)},capture_output=True,text=True,check=True,timeout=10)
            self.assertEqual(json.loads(result.stdout),['-machine','virt,accel=tcg','-drive','file=/tmp/with spaces'])

    def test_user_hosts_and_foreign_workspaces_stop_before_any_download_or_engine(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve()
            base={'GITHUB_ACTIONS':'true','RUNNER_OS':'macOS','GITHUB_WORKSPACE':str(root),'RUNNER_TEMP':str(root)}
            with patch.object(engine,'host_key',return_value='macos-arm64'),patch.dict(os.environ,base):
                self.assertEqual(engine.runner(root),root)
                for changed in ({'GITHUB_ACTIONS':'false'},{'RUNNER_OS':'Linux'},{'GITHUB_WORKSPACE':str(root/'other')},{'RUNNER_TEMP':'relative'}):
                    with patch.dict(os.environ,changed),self.assertRaises(ValueError):engine.runner(root)
            with patch.object(engine,'inputs') as download,self.assertRaises(ValueError):engine.build(root,root/'sdk')
            download.assert_not_called()
            self.assertFalse((root/'dist').exists())

    def test_qemu_version_and_firmware_are_checked_before_start(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'config').mkdir();(root/'config/macos-factory.json').write_text('{"format":1,"qemu_version":"11.1.3"}')
            binary=root/'brew/bin/qemu-system-aarch64';binary.parent.mkdir(parents=True);binary.touch()
            firmware=binary.parent.parent/'share/qemu/edk2-aarch64-code.fd';firmware.parent.mkdir(parents=True);firmware.write_bytes(b'firmware')
            with patch.object(engine.shutil,'which',return_value=str(binary)),patch.object(engine.subprocess,'check_output',return_value='QEMU emulator version 11.1.3 (Homebrew)\n') as execute:
                _,actual,code=engine.qemu(root);self.assertEqual(actual,binary.resolve());self.assertEqual(code,firmware.resolve())
                execute.assert_called_once_with([binary.resolve(),'--version'],text=True,timeout=15)
                firmware.unlink()
                with self.assertRaisesRegex(ValueError,'firmware'):engine.qemu(root)
            with patch.object(engine.shutil,'which',return_value=str(binary)),patch.object(engine.subprocess,'check_output',return_value='QEMU emulator version 12.0.0\n'),self.assertRaisesRegex(ValueError,'pinned'):engine.qemu(root)

    def test_factory_definition_is_qemu_9p_with_pinned_firmware_and_explicit_store(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve();work=root/'work';firmware=root/'firmware';firmware.write_bytes(b'arm firmware')
            runtime={'packages':work/'packages','image':work/'image.iso','image_sha256':'a'*64,'arch':'aarch64','vm_type':'qemu'}
            before=dict(runtime)
            value=engine.definition(root,work,runtime,{'engine_memory':'3GiB','engine_cpus':3,'engine_disk':'48GiB'},firmware)
            self.assertEqual((value['vmType'],value['mountType'],value['cpuType']),('qemu','9p',{'aarch64':'max'}))
            self.assertEqual(value['mounts'],[{'location':str(root),'writable':True},{'location':str(work),'writable':True}])
            self.assertEqual(value['firmware']['images'][0]['location'],str(firmware))
            self.assertIn('containerd-snapshotter',value['provision'][1]['script'])
            self.assertFalse(value['ssh']['forwardAgent']);self.assertEqual(runtime,before)

    @unittest.skipUnless(os.name=='posix','Hosted Mac factory uses POSIX executables')
    def test_real_docker_wrapper_forwards_arguments_to_only_the_owned_lima(self):
        with tempfile.TemporaryDirectory(prefix='factory wrapper ') as temp:
            root=Path(temp).resolve();work=root/'work';work.mkdir();lima=root/'fake lima';program=root/'lima.py'
            program.write_text('import json,os,sys;print(json.dumps({"argv":sys.argv[1:],"home":os.environ["LIMA_HOME"]}))')
            lima.write_text('#!/bin/sh\nexec '+shlex.quote(sys.executable)+' '+shlex.quote(str(program))+' "$@"\n');lima.chmod(0o755)
            with patch.dict(os.environ,{'DOCKER_HOST':'foreign','DOCKER_CONTEXT':'foreign','DOCKER_TLS_VERIFY':'1','DOCKER_CERT_PATH':'foreign'}):
                env=engine.wrappers(root,work,{'command':lima},root/'qemu')
            self.assertFalse({'DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH'} & env.keys())
            result=subprocess.run([work/'bin/docker','image','inspect','name with spaces'],env=env,capture_output=True,text=True,check=True,timeout=10)
            value=json.loads(result.stdout)
            self.assertEqual(value['home'],str(work/'v'))
            self.assertEqual(value['argv'],['shell','--workdir',str(root),'factory','sudo','docker','image','inspect','name with spaces'])

    def scenario(self,root,*,fail=None,cleanup_failure=False,complete=True):
        calls=[];policy={'engine_memory':'3GiB','engine_cpus':3,'engine_disk':'48GiB'}
        def execute(argv,source,env,log,**options):
            calls.append([str(value) for value in argv]);phase=str(argv[1])
            if phase=='start':(Path(env['LIMA_HOME'])/'factory').mkdir(parents=True)
            if phase=='stop' and cleanup_failure:raise OSError('fixture cleanup failure')
            if phase==fail or fail=='factory' and phase.endswith('macos_client_factory.py'):raise ValueError('fixture primary failure')
            if phase.endswith('macos_client_factory.py'):
                (root/'dist/macos-full-factory-report.json').write_text(json.dumps({'artifact':{'delivery':{'complete_device_bundle':complete},'file':'full.dmg'}}))
        with ExitStack() as stack:
            for target,value in [('runner',root),('qemu',(policy,root/'qemu',root/'firmware')),('inputs',{'command':root/'limactl'}),('definition',{})]:
                stack.enter_context(patch.object(engine,target,return_value=value))
            stack.enter_context(patch('macos_client_factory.compiler_environment',return_value={}))
            stack.enter_context(patch('android_factory_stock.reserve'))
            stack.enter_context(patch.object(engine,'verify_software_engine'))
            stack.enter_context(patch.object(engine,'command',side_effect=execute))
            try:return engine.build(root,root/'sdk'),calls
            finally:self.calls=calls

    def test_full_factory_handoff_and_owned_engine_cleanup(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve();receipt,calls=self.scenario(root)
            self.assertTrue(receipt['complete_device_bundle']);self.assertTrue(receipt['engine_cleanup_verified'])
            self.assertEqual([row[1] for row in calls],['start','info',str(root/'scripts/macos_client_factory.py'),'stop','delete'])
            self.assertEqual(calls[2][-2:],['--compiler-sdk',str(root/'sdk')])
            self.assertEqual(list(root.glob('af-*')),[])

    def test_failed_factory_preserves_primary_and_stops_only_its_engine(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve()
            with self.assertRaisesRegex(ValueError,'primary failure'):self.scenario(root,fail='factory')
            receipt=json.loads((root/'dist/macos-factory-engine-report.json').read_text())
            self.assertEqual(receipt['phase'],'complete-client');self.assertFalse(receipt['complete_device_bundle'])
            self.assertTrue(receipt['engine_cleanup_verified']);self.assertEqual([v[1] for v in self.calls][-2:],['stop','delete'])
            self.assertEqual(list(root.glob('af-*')),[])

    def test_failed_start_is_not_retried_and_partial_engine_is_cleaned(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve()
            with self.assertRaisesRegex(ValueError,'primary failure'):self.scenario(root,fail='start')
            receipt=json.loads((root/'dist/macos-factory-engine-report.json').read_text())
            self.assertFalse(receipt['engine_ready']);self.assertEqual([v[1] for v in self.calls],['start','stop','delete'])
            self.assertEqual(list(root.glob('af-*')),[])

    def test_cleanup_failure_keeps_owned_state_and_both_failures(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve()
            with self.assertRaises(BaseExceptionGroup) as caught:self.scenario(root,fail='factory',cleanup_failure=True)
            self.assertEqual([str(v) for v in caught.exception.exceptions],['fixture primary failure','fixture cleanup failure'])
            receipt=json.loads((root/'dist/macos-factory-engine-report.json').read_text())
            self.assertFalse(receipt['engine_cleanup_verified'])
            self.assertEqual(len(list(root.glob('af-*/owner.json'))),1)
            self.assertNotIn('delete',[v[1] for v in self.calls])

    def test_foreign_marker_prevents_stop_or_delete(self):
        with tempfile.TemporaryDirectory() as temp:
            work=Path(temp);(work/'owner.json').write_text('{"name":"foreign"}')
            with patch.object(engine,'command') as execute,self.assertRaisesRegex(ValueError,'owner changed'):
                engine.stop_owned(work,{'command':work/'limactl'},{'LIMA_HOME':str(work/'v')},None)
            execute.assert_not_called()

    def test_incomplete_package_cannot_qualify_the_factory(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve()
            with self.assertRaisesRegex(ValueError,'complete client'):self.scenario(root,complete=False)
            receipt=json.loads((root/'dist/macos-factory-engine-report.json').read_text())
            self.assertFalse(receipt['complete_device_bundle']);self.assertTrue(receipt['engine_cleanup_verified'])

    def test_actual_engine_argv_must_identify_tcg_before_full_build(self):
        with tempfile.TemporaryDirectory() as temp:
            work=Path(temp);folder=work/'v/factory';folder.mkdir(parents=True);(folder/'qemu.pid').write_text('1234')
            executable=work/'qemu-system-aarch64'
            with patch.object(engine.subprocess,'check_output',return_value=str(executable)+' -machine virt,accel=tcg,highmem=on') as ps:
                engine.verify_software_engine(work,executable)
                ps.assert_called_once_with(['ps','-p','1234','-o','command='],text=True,timeout=15)
            for args in (str(executable)+' -machine virt,accel=hvf','foreign -machine virt,accel=tcg'):
                with patch.object(engine.subprocess,'check_output',return_value=args),self.assertRaisesRegex(ValueError,'software acceleration'):
                    engine.verify_software_engine(work,executable)

    def test_command_preserves_both_body_and_cleanup_failures(self):
        primary=RuntimeError('fixture command failure');cleanup=PermissionError('fixture group cleanup failure')
        child=SimpleNamespace(pid=4321,returncode=None,wait=Mock(side_effect=primary))
        with patch.object(engine.subprocess,'Popen',return_value=child),patch.object(engine.os,'killpg',side_effect=cleanup):
            with self.assertRaises(BaseExceptionGroup) as caught:engine.command(['fixture'],Path('.'),{},None)
        self.assertEqual(caught.exception.exceptions,(primary,cleanup))

    def test_completed_command_never_signals_a_released_pid(self):
        child=SimpleNamespace(pid=4321,returncode=0,wait=Mock(return_value=0))
        with patch.object(engine.subprocess,'Popen',return_value=child),patch.object(engine.os,'killpg') as signal:
            engine.command(['fixture'],Path('.'),{},None)
        signal.assert_not_called()

    @unittest.skipUnless(os.name=='posix','Hosted Mac factory owns POSIX process groups')
    def test_real_timeout_reaps_the_owned_command_and_its_waited_child(self):
        script='''import json,os,pathlib,signal,subprocess,sys,time
child=subprocess.Popen([sys.executable,'-I','-S','-B','-c','import time;time.sleep(60)'])
def stopped(*args):
 child.wait(timeout=5)
 raise SystemExit(143)
signal.signal(signal.SIGTERM,stopped)
pathlib.Path(sys.argv[1]).write_text(json.dumps([os.getpid(),child.pid]))
time.sleep(60)
'''
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);record=root/'pids.json';children=[];original=subprocess.Popen
            def spawn(*args,**kwargs):
                child=original(*args,**kwargs);children.append(child);return child
            try:
                with (root/'log').open('w') as log,patch.object(engine.subprocess,'Popen',side_effect=spawn):
                    with self.assertRaises(subprocess.TimeoutExpired):
                        engine.command([sys.executable,'-I','-S','-B','-c',script,record],root,dict(os.environ),log,timeout=1)
                self.assertEqual(len(children),1);self.assertEqual(children[0].returncode,143)
                parent,child=json.loads(record.read_text());self.assertEqual(parent,children[0].pid)
                with self.assertRaises(ProcessLookupError):os.kill(child,0)
            finally:
                for child in children:
                    if child.returncode is None:
                        os.killpg(child.pid,9);child.wait(timeout=5)


if __name__=='__main__':unittest.main()
