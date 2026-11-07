"""Disposable qualification ownership and truthful evidence contracts."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


class QualificationScopeTests(unittest.TestCase):
    def test_local_or_self_hosted_execution_is_rejected_before_docker(self):
        from wsl_runtime_fixture import require_disposable
        for env in ({},{'GITHUB_ACTIONS':'true','RUNNER_ENVIRONMENT':'self-hosted','AI_PHONE_DISPOSABLE_QUALIFICATION':'1'}):
            with patch.dict(os.environ,env,clear=True):
                with self.assertRaisesRegex(ValueError,'GitHub-hosted'):require_disposable()
        with patch.dict(os.environ,{'GITHUB_ACTIONS':'true','RUNNER_ENVIRONMENT':'github-hosted','RUNNER_OS':'Linux','AI_PHONE_DISPOSABLE_QUALIFICATION':'1'},clear=True):require_disposable()

    def test_fixture_is_privileged_but_exposes_no_host_bind_or_socket(self):
        from wsl_runtime_fixture import create_command
        command=create_command('a'*32,'sha256:'+'b'*64)
        self.assertIn('--privileged',command);self.assertIn('--network=none',command)
        self.assertIn('--cgroupns=private',command)
        self.assertFalse(any('/var/run/docker.sock' in str(arg) or 'type=bind' in str(arg) for arg in command))
        self.assertIn('type=volume,source=ai-phone-wsl-q-'+'a'*32+',target=/var/lib/aiphone',command)

    def test_mismatched_owner_is_never_removed(self):
        from wsl_runtime_fixture import cleanup
        calls=[]
        def docker(args,**kwargs):
            calls.append(args)
            return json.dumps([{'Id':'c'*64,'Config':{'Labels':{'dev.aiphone.qualification':'foreign'}},'Name':'/foreign'}])
        result=cleanup('a'*32,{'container':'c'*64},docker)
        self.assertFalse(result['container']);self.assertEqual(len(calls),1)

    def test_public_evidence_rejects_extra_fields_and_false_completion(self):
        from wsl_runtime_fixture import worker_result
        record={'format':1,'nonce':'a'*32,'phase':1,'status':'passed','stage':'stopped','checks':{'assistant':True,'web':True,'channels':True,'runner_endpoint':True,'database_roles':True,'redis':True,'desktop_frame':True,'work_file':True,'database_marker':True,'desktop_registered':True,'desktop_media':True},'frame':{'width':1280,'height':800,'sha256':'b'*64},'windows_boot_verified':False,'phone_ready':False}
        from wsl_initialize_diagnostics import initial
        record['initialization']={**initial(),'step':'complete'}
        self.assertEqual(worker_result(record,'a'*32,1),record)
        for extra in ({**record,'token':'secret'},{**record,'windows_boot_verified':True},{**record,'phase':2}):
            with self.assertRaises(ValueError):worker_result(extra,'a'*32,1)

    def test_runtime_capacity_keeps_reserve_and_real_expansion_budget(self):
        from wsl_runtime_fixture import runtime_capacity
        with patch('wsl_runtime_fixture.shutil.disk_usage') as usage:
            usage.return_value.free=4*1024**3
            with self.assertRaises(ValueError):runtime_capacity(Path('.'),1000,2000,3000)
            usage.return_value.free=20*1024**3
            expected=4*1024**3+2*1000+2000+2*3000+1024**3
            self.assertEqual(runtime_capacity(Path('.'),1000,2000,3000),expected)
            usage.return_value.free=expected
            self.assertEqual(runtime_capacity(Path('.'),1000,2000,3000),expected)


class QualificationLifecycleTests(unittest.TestCase):
    def test_second_phase_failure_cleans_owned_resources_and_saves_partial_checks(self):
        import wsl_runtime_fixture as fixture
        first={'status':'passed','stage':'stopped'}
        second={'status':'failed','stage':'persistence'}
        def materialize(_work,_nonce,resources,_worker):resources['container']='c'*64;return 'c'*64
        with tempfile.TemporaryDirectory() as folder,patch.object(fixture,'require_disposable'),patch.object(fixture,'native_worker',return_value=Path('/fixture/native-worker')),patch.object(fixture,'checked_inputs',return_value={'rootfs_sha256':'a'*64}),patch.object(fixture,'materialize',side_effect=materialize),patch.object(fixture,'run_phase',side_effect=[first,second]) as run,patch.object(fixture,'cleanup',return_value={'container':True}) as clean:
            with self.assertRaisesRegex(ValueError,'did not pass'):fixture.qualify_runtime(Path(folder),Path('source'),Path('images'))
            self.assertEqual([call.args[3] for call in run.call_args_list],[1,2])
            clean.assert_called_once()
            result=json.loads((Path(folder)/'runtime-receipt.json').read_text())
            self.assertFalse(result['passed']);self.assertEqual(result['stage'],'phase_2')
            self.assertEqual(result['phases'][1]['stage'],'persistence')

    def test_cleanup_failure_cannot_report_success(self):
        import wsl_runtime_fixture as fixture
        def materialize(_work,_nonce,resources,_worker):resources['container']='c'*64;return 'c'*64
        with tempfile.TemporaryDirectory() as folder,patch.object(fixture,'require_disposable'),patch.object(fixture,'native_worker',return_value=Path('/fixture/native-worker')),patch.object(fixture,'checked_inputs',return_value={}),patch.object(fixture,'materialize',side_effect=materialize),patch.object(fixture,'run_phase',return_value={'status':'passed'}),patch.object(fixture,'cleanup',return_value={'container':False}):
            with self.assertRaisesRegex(ValueError,'cleanup'):fixture.qualify_runtime(Path(folder),Path('source'),Path('images'))
            result=json.loads((Path(folder)/'runtime-receipt.json').read_text())
            self.assertFalse(result['passed']);self.assertEqual(result['stage'],'cleanup')

    def test_outer_qualification_cannot_pass_only_a_built_component(self):
        import wsl_backend_qualification as qualification
        with tempfile.TemporaryDirectory() as folder,patch('wsl_runtime_fixture.require_disposable'),patch.object(qualification,'assemble',return_value=({},Path(folder)/'work',Path(folder)/'images')),patch('wsl_runtime_fixture.qualify_runtime',side_effect=ValueError('fixture failure')) as runtime:
            with self.assertRaisesRegex(ValueError,'fixture failure'):qualification.qualify(Path(folder))
            runtime.assert_called_once()
            result=json.loads((Path(folder)/'dist/wsl-backend-qualification.json').read_text())
            self.assertFalse(result['passed']);self.assertEqual(result['stage'],'runtime')


class QualificationInputsTests(unittest.TestCase):
    def test_actual_source_image_and_rootfs_bytes_are_checked_before_import(self):
        from test_wsl_backend import WslSourceTests,rootfs
        import subprocess
        from test_release import native_profile_program
        from wsl_backend import publish_capsule
        from artifact_io import digest
        from wsl_runtime_fixture import checked_inputs
        with tempfile.TemporaryDirectory() as folder,patch('wsl_runtime_fixture.shutil.disk_usage') as usage:
            root=Path(folder);usage.return_value.free=20*1024**3
            source=WslSourceTests().payload(root)
            producer=Path(native_profile_program()).with_name('image-test-engine'+('.exe' if os.name=='nt' else ''))
            subprocess.run([str(producer),'--fixture-capsule',str(root),'amd64','wsl-inputs','gzip'],capture_output=True,check=True)
            images=root/'images.payload';body=root/'rootfs.tar.gz';rootfs(body)
            publish_capsule(body,root/'wsl-backend.payload',{'source_sha256':digest(source),'images_sha256':digest(images)})
            proof=checked_inputs(root,source,images)
            self.assertEqual(proof['rootfs_sha256'],digest(body))
            source.write_bytes(source.read_bytes()+b'changed')
            with self.assertRaisesRegex(ValueError,'inputs differ'):checked_inputs(root,source,images)
            body.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'Rootfs changed'):checked_inputs(root,source,images)

    def test_fixture_creation_and_removal_only_use_returned_owned_identities(self):
        import wsl_runtime_fixture as fixture
        nonce='a'*32;image='sha256:'+'b'*64;container='c'*64;volume=fixture.identity(nonce);calls=[]
        def docker(args,**_kwargs):
            calls.append(args)
            if args[:2]==['image','import']:return image
            if args[:2]==['volume','create']:return volume
            if args[0]=='create':return container
            if args[1]=='inspect':
                kind,_,reference=args
                return json.dumps([{'Name':reference,'Id':reference,'Labels':{fixture.LABEL:nonce},'Config':{'Labels':{fixture.LABEL:nonce}}}])
            if args[0]=='cp':return ''
            if args[1]=='rm':return ''
            self.fail('Unexpected fixture command')
        resources={}
        self.assertEqual(fixture.materialize(Path('own-work'),nonce,resources,Path('/fixture/native-worker'),docker),container)
        self.assertEqual(resources,{'image':image,'volume':volume,'container':container})
        self.assertIn(['cp',Path('/fixture/native-worker'),container+':/usr/local/libexec/aiphone-qualification'],calls)
        self.assertEqual(fixture.cleanup(nonce,resources,docker),{'container':True,'image':True,'volume':True})
        removals=[args[-1] for args in calls if args[1]=='rm']
        self.assertEqual(removals,[container,image,volume])
        self.assertFalse(any('export' in args or 'prune' in args for args in calls))
