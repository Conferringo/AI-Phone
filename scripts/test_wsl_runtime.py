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
        record={'format':1,'nonce':'a'*32,'phase':1,'status':'passed','stage':'stopped','checks':{'assistant':True,'web':True,'channels':True,'database_roles':True,'redis':True,'desktop_frame':True,'work_file':True,'database_marker':True,'desktop_registered':True,'desktop_media':True},'frame':{'width':1280,'height':800,'sha256':'b'*64},'windows_boot_verified':False,'phone_ready':False}
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


class RuntimeProbesTests(unittest.TestCase):
    def test_frame_probe_decodes_real_png_and_rejects_header_only(self):
        import struct,zlib
        from wsl_runtime_probes import frame_evidence
        def chunk(kind,data):return struct.pack('!I',len(data))+kind+data+struct.pack('!I',zlib.crc32(kind+data))
        raw=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!IIBBBBB',2,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\x01\x02\x03\x04\x05\x06'))+chunk(b'IEND',b'')
        self.assertEqual(frame_evidence(raw)['width'],2)
        with self.assertRaises(ValueError):frame_evidence(raw[:24])

    def test_qualification_marker_never_adopts_other_fixture(self):
        from wsl_runtime_worker import next_phase
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'marker'
            self.assertEqual(next_phase(path,'a'*32),1)
            path.write_text(json.dumps({'nonce':'b'*32,'phase':1}))
            with self.assertRaisesRegex(ValueError,'owner'):next_phase(path,'a'*32)
            self.assertEqual(json.loads(path.read_text())['nonce'],'b'*32)


class QualificationLifecycleTests(unittest.TestCase):
    def test_second_phase_failure_cleans_owned_resources_and_saves_partial_checks(self):
        import wsl_runtime_fixture as fixture
        first={'status':'passed','stage':'stopped'}
        second={'status':'failed','stage':'persistence'}
        def materialize(_work,_nonce,resources):resources['container']='c'*64;return 'c'*64
        with tempfile.TemporaryDirectory() as folder,patch.object(fixture,'require_disposable'),patch.object(fixture,'checked_inputs',return_value={'rootfs_sha256':'a'*64}),patch.object(fixture,'materialize',side_effect=materialize),patch.object(fixture,'run_phase',side_effect=[first,second]) as run,patch.object(fixture,'cleanup',return_value={'container':True}) as clean:
            with self.assertRaisesRegex(ValueError,'did not pass'):fixture.qualify_runtime(Path(folder),Path('source'),Path('images'))
            self.assertEqual([call.args[3] for call in run.call_args_list],[1,2])
            clean.assert_called_once()
            result=json.loads((Path(folder)/'runtime-receipt.json').read_text())
            self.assertFalse(result['passed']);self.assertEqual(result['stage'],'phase_2')
            self.assertEqual(result['phases'][1]['stage'],'persistence')

    def test_cleanup_failure_cannot_report_success(self):
        import wsl_runtime_fixture as fixture
        def materialize(_work,_nonce,resources):resources['container']='c'*64;return 'c'*64
        with tempfile.TemporaryDirectory() as folder,patch.object(fixture,'require_disposable'),patch.object(fixture,'checked_inputs',return_value={}),patch.object(fixture,'materialize',side_effect=materialize),patch.object(fixture,'run_phase',return_value={'status':'passed'}),patch.object(fixture,'cleanup',return_value={'container':False}):
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

    def test_mjpeg_probe_reads_an_actual_bounded_stream_and_rejects_eof(self):
        import io
        from wsl_runtime_probes import first_jpeg
        self.assertEqual(first_jpeg(io.BytesIO(b'--phone\r\n\xff\xd8abc\xff\xd9\r\n')),b'\xff\xd8abc\xff\xd9')
        with self.assertRaisesRegex(ValueError,'complete frame'):first_jpeg(io.BytesIO(b'--phone\r\npartial'))


class RuntimeHealthContractTests(unittest.TestCase):
    def test_requests_use_protobuf_auth_and_local_entry_origin_without_proxy(self):
        import http.server,threading,urllib.request
        from wsl_runtime_probes import request
        seen=[]
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                seen.append((self.path,dict(self.headers),self.rfile.read(int(self.headers.get('content-length',0)))))
                self.send_response(200);self.send_header('content-type','application/x-protobuf');self.end_headers();self.wfile.write(b'\x08\x01')
            def log_message(self,*_):pass
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        try:
            opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
            with request(opener,'http://127.0.0.1:'+str(server.server_port)+'/fixture',token='fixture-token',data=b'') as response:
                self.assertEqual(response.read(),b'\x08\x01')
            self.assertEqual(seen[0][1]['Authorization'],'Bearer fixture-token')
            self.assertEqual(seen[0][1]['Content-Type'],'application/x-protobuf')
            with patch.object(opener,'open') as opening:request(opener,'http://127.0.0.1:17860/api/auth/enter',data=b'')
            self.assertEqual(opening.call_args.args[0].get_header('Origin'),'http://127.0.0.1:17860')
        finally:server.shutdown();server.server_close();thread.join(timeout=2)

    def test_health_probe_refuses_wrong_content_type_and_empty_html(self):
        import io
        from wsl_runtime_probes import wait_response
        for headers,body,content in (({'content-type':'text/html'},b'fake','application/x-protobuf'),({'content-type':'text/html'},b'','text/html')):
            response=io.BytesIO(body);response.status=200;response.headers=headers
            with patch('wsl_runtime_probes.request',return_value=response):
                with self.assertRaises(ValueError):wait_response(None,'fixture','fixture',b'',content,'token')
        response=io.BytesIO(b'\x08\x01');response.status=200;response.headers={'content-type':'application/x-protobuf'}
        with patch('wsl_runtime_probes.request',return_value=response):wait_response(None,'assistant','fixture',b'','application/x-protobuf','token')

    def test_initial_default_protobuf_state_is_a_valid_empty_message(self):
        import io
        from wsl_runtime_probes import wait_response
        response=io.BytesIO(b'');response.status=200;response.headers={'content-type':'application/x-protobuf'}
        with patch('wsl_runtime_probes.request',return_value=response):
            wait_response(None,'assistant','fixture',b'','application/x-protobuf','token')

    def test_database_probe_rejects_privileged_role_and_unavailable_redis(self):
        import wsl_runtime_probes as probes
        spec={'argv':['fixture-assistant'],'environment':{}}
        with patch('prebuilt_services.launch_spec',return_value=spec):
            with patch.object(probes,'execute',return_value=b'{"runtime":"postgres","privileged":true,"ddl":true}'):
                with self.assertRaisesRegex(ValueError,'restricted'):probes.database_roles(Path('.'))
            good=b'{"runtime":"ai_phone_runtime","privileged":false,"ddl":false}'
            with patch.object(probes,'execute',side_effect=[good,b'FAIL']):
                with self.assertRaisesRegex(ValueError,'Redis'):probes.database_roles(Path('.'))
            with patch.object(probes,'execute',side_effect=[good,b'PONG\n']):self.assertEqual(probes.database_roles(Path('.')),{'database_roles':True,'redis':True})

    def test_only_container_pid_one_is_admitted_to_nested_cgroup_setup(self):
        import wsl_runtime_worker as worker
        with patch.dict(os.environ,{'AI_PHONE_WSL_QUALIFICATION_NONCE':'a'*32,'container':'docker'}),patch.object(worker.Path,'is_file',return_value=True),patch.object(worker.os,'getpid',return_value=2):
            with self.assertRaisesRegex(ValueError,'PID 1'):worker.fixture_identity()
        with patch.dict(os.environ,{'AI_PHONE_WSL_QUALIFICATION_NONCE':'a'*32,'container':'docker'}),patch.object(worker.Path,'is_file',return_value=True),patch.object(worker.os,'getpid',return_value=1):self.assertEqual(worker.fixture_identity(),'a'*32)

    def test_cgroup_setup_only_writes_the_supplied_private_mount(self):
        import wsl_runtime_worker as worker
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'cgroup.controllers').write_text('cpu memory pids');(root/'cgroup.procs').write_text('1\n')
            with patch.object(worker,'Path',return_value=root),patch.object(worker.subprocess,'run') as mount:worker.nested_cgroups()
            self.assertEqual((root/'aiphone-qualification/cgroup.procs').read_text(),'1')
            self.assertEqual((root/'cgroup.subtree_control').read_text(),'+cpu +memory +pids')
            self.assertEqual(mount.call_args.args[0],['/bin/mount','--make-rshared','/'])

    def test_failed_worker_retains_fixed_stage_and_does_not_publish_phase_marker(self):
        import wsl_runtime_worker as worker
        def fail(receipt,*_):receipt['stage']='desktop';raise ValueError('private credential must never be serialized')
        with tempfile.TemporaryDirectory() as folder,patch.object(worker,'STATE',Path(folder)),patch.object(worker,'fixture_identity',return_value='a'*32),patch.object(worker,'perform',side_effect=fail):
            self.assertEqual(worker.main(),1)
            result=json.loads((Path(folder)/'qualification-result.json').read_text())
            self.assertEqual(result['stage'],'desktop');self.assertEqual(result['status'],'failed')
            self.assertNotIn('credential',json.dumps(result));self.assertFalse((Path(folder)/'qualification-marker.json').exists())


class QualificationInputsTests(unittest.TestCase):
    def test_actual_source_image_and_rootfs_bytes_are_checked_before_import(self):
        from test_wsl_backend import WslSourceTests,rootfs
        from test_wsl_backend_review import WslOwnershipChainTests
        from wsl_backend import publish_capsule
        from artifact_io import digest
        from wsl_runtime_fixture import checked_inputs
        with tempfile.TemporaryDirectory() as folder,patch('wsl_runtime_fixture.shutil.disk_usage') as usage:
            root=Path(folder);usage.return_value.free=20*1024**3
            source=WslSourceTests().payload(root);WslOwnershipChainTests().image_payload(root)
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
            if args[1]=='rm':return ''
            self.fail('Unexpected fixture command')
        resources={}
        self.assertEqual(fixture.materialize(Path('own-work'),nonce,resources,docker),container)
        self.assertEqual(resources,{'image':image,'volume':volume,'container':container})
        self.assertEqual(fixture.cleanup(nonce,resources,docker),{'container':True,'image':True,'volume':True})
        removals=[args[-1] for args in calls if args[1]=='rm']
        self.assertEqual(removals,[container,image,volume])
        self.assertFalse(any('export' in args or 'prune' in args for args in calls))

    def test_web_media_probe_demands_authenticated_stream_and_guest_decode(self):
        import io
        import wsl_runtime_probes as probes
        for content_type in ('application/octet-stream','Application/Octet-Stream; charset=binary'):
            with self.subTest(content_type=content_type):
                login=io.BytesIO(b'ok');login.status=200
                media=io.BytesIO(b'--phone\r\n\xff\xd8fixture\xff\xd9\r\n');media.status=200;media.headers={'content-type':content_type}
                with patch.object(probes,'request',side_effect=[login,media]) as request,patch.object(probes,'execute',return_value=b'png') as execute,patch.object(probes,'frame_evidence',return_value={'width':1280,'height':800}):
                    self.assertTrue(probes.desktop_media(Path('.'),'device-fixture'))
                    self.assertEqual(request.call_args_list[0].args[1],'http://127.0.0.1:17860/api/auth/enter')
                    self.assertIn('/api/desktop/media?device_id=device-fixture&raw=1',request.call_args_list[1].args[1])
                    self.assertEqual(execute.call_args.kwargs['input'],b'\xff\xd8fixture\xff\xd9')
                    self.assertIn('ai-phone-desktop',execute.call_args.args[1])
        denied=io.BytesIO();denied.status=403
        with patch.object(probes,'request',return_value=denied):
            with self.assertRaisesRegex(ValueError,'Local web entry'):probes.desktop_media(Path('.'),'device-fixture')

    def test_raw_web_media_probe_rejects_a_different_transport_contract(self):
        import io
        import wsl_runtime_probes as probes
        login=io.BytesIO(b'ok');login.status=200
        media=io.BytesIO(b'--phone\r\n\xff\xd8fixture\xff\xd9\r\n');media.status=200;media.headers={'content-type':'text/html'}
        with patch.object(probes,'request',side_effect=[login,media]),patch.object(probes,'execute') as execute:
            with self.assertRaisesRegex(ValueError,'media is unavailable'):probes.desktop_media(Path('.'),'device-fixture')
            execute.assert_not_called()


class WorkingFilePersistenceTests(unittest.TestCase):
    def test_real_exclusive_file_creation_and_readback_never_overwrite_existing_work(self):
        import subprocess,sys
        import wsl_runtime_probes as probes
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);files=root/'data/desktop/home/Files';files.mkdir(parents=True)
            def relocated_guest(_root,args,**kwargs):
                self.assertEqual(args[:6],['docker','exec','-i','-u','ai','ai-phone-desktop'])
                script=args[-1].replace('/home/ai/Files',str(files))
                return subprocess.run([sys.executable,'-c',script],input=kwargs['input'],capture_output=True,check=True).stdout
            with patch.object(probes,'execute',side_effect=relocated_guest):
                self.assertTrue(probes.work_file(root,'a'*32,1))
                path=files/('qualification-'+'a'*32+'.txt');original=path.read_bytes()
                self.assertTrue(probes.work_file(root,'a'*32,2));self.assertEqual(path.read_bytes(),original)
                with self.assertRaises(subprocess.CalledProcessError):probes.work_file(root,'a'*32,1)
                self.assertEqual(path.read_bytes(),original)
                path.write_bytes(b'user fixture change')
                with self.assertRaises(subprocess.CalledProcessError):probes.work_file(root,'a'*32,2)
                self.assertEqual(path.read_bytes(),b'user fixture change')


if __name__=='__main__':unittest.main()
