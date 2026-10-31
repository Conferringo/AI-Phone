"""Native-worker contracts using isolated files and real fixture processes."""
import io
import json
from pathlib import Path
import subprocess
import shlex
import sys
import tempfile
import unittest
from unittest.mock import patch
from client_host import check_request, sdk_licenses, verify_container_ownership, installation_reserve, write_container_wrapper
from install_state import InstallState


def python_command(path, source):
    # A POSIX shebang cannot quote an interpreter path containing spaces.
    path.write_text('#!/bin/sh\nexec ' + shlex.quote(sys.executable) + ' -c ' + shlex.quote(source) + ' "$@"\n')


class NativeHostTests(unittest.TestCase):
    def test_large_download_is_streamed_and_bad_replacement_preserves_cached_file(self):
        import hashlib
        from client_host import download
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'runtime';path.write_bytes(b'original')
            expected=hashlib.sha256(b'complete').hexdigest()
            with patch.object(Path,'read_bytes',side_effect=AssertionError('Unbounded dependency read')):
                opener=unittest.mock.Mock()
                opener.open.return_value=io.BytesIO(b'wrong')
                with patch('client_host.urllib.request.build_opener',return_value=opener):
                    with self.assertRaisesRegex(ValueError,'checksum'):download({'url':'https://example.test/runtime','sha256':expected},path)
                with path.open('rb') as original:self.assertEqual(original.read(),b'original')
                self.assertFalse(path.with_suffix('.partial').exists())
                opener.open.return_value=io.BytesIO(b'complete')
                with patch('client_host.urllib.request.build_opener',return_value=opener):
                    self.assertEqual(download({'url':'https://example.test/runtime','sha256':expected},path),path)
            self.assertEqual(path.read_bytes(),b'complete')

    def test_first_engine_creation_does_not_query_a_missing_instance(self):
        from client_host import activate_container_engine
        import os
        with tempfile.TemporaryDirectory(dir='/tmp',prefix='ae-') as temporary:
            base=Path(temporary);home=base/'install';root=home/'releases'/'version';root.mkdir(parents=True);data=home/'user/data';data.mkdir(parents=True);(root/'data').symlink_to(data,target_is_directory=True)
            command=home/'limactl';command.write_text('#!/bin/sh\nexit 0\n');command.chmod(0o755)
            def query(args,**kwargs):
                return subprocess.CompletedProcess(args,1 if 'ai-phone' in args else 0,stdout='',stderr='unmatched instances')
            with patch('managed_engine.Path.home',return_value=base),patch.dict(os.environ,{'PATH':'/usr/bin:/bin'},clear=True),patch('client_host.subprocess.run',side_effect=query),patch('client_host.execute') as execute:
                activate_container_engine(root,home,command,{'fixture':True})
                self.assertIn('--name=ai-phone',execute.call_args_list[0].args[0])

    def test_complete_host_never_adopts_global_docker(self):
        from client_host import container_engine
        import os
        runtime={'command':Path('/private/runtime/lima')}
        with patch.dict(os.environ,{'AI_PHONE_DESKTOP_MEMORY':'2560m'},clear=True),patch('container_host_runtime.selection',return_value=runtime),patch('container_host_runtime.definition',return_value={'local':True}),patch('host_platform.host_policy',return_value=({},{})),patch('client_host.activate_container_engine') as activate,patch('client_host.subprocess.run') as run,patch('client_host.shutil.which',return_value='/usr/local/bin/docker'):
            container_engine(Path('/private/root'),Path('/private'),{},lambda *_:None)
            activate.assert_called_once_with(Path('/private/root'),Path('/private'),runtime['command'],{'local':True})
            run.assert_not_called()

    def test_wrapper_does_not_overwrite_an_unowned_file_or_follow_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            home=Path(temporary);folder=home/'bin';folder.mkdir();wrapper=folder/'docker';wrapper.write_text('unowned')
            with self.assertRaises(ValueError):write_container_wrapper(home,home,home/'limactl')
            self.assertEqual(wrapper.read_text(),'unowned');wrapper.unlink();target=home/'original';target.write_text('keep');wrapper.symlink_to(target)
            with self.assertRaises(ValueError):write_container_wrapper(home,home,home/'limactl')
            self.assertEqual(target.read_text(),'keep')

    def test_complete_bundled_toolchain_does_not_run_a_package_manager(self):
        from client_host import selected_tools,install_tools
        tools=selected_tools(['python@3.13.18','node@22.23.4','rust@1.96.0','java@temurin-17'],True,True,('python','node'),True)
        self.assertEqual(tools,[])
        with patch('client_host.execute') as execute:
            install_tools(Path('/isolated'),None,tools,lambda *_:self.fail('No tool installation is needed'))
        execute.assert_not_called()
    def test_owned_vm_keeps_architecture_resources_and_quoted_arguments(self):
        import os
        with tempfile.TemporaryDirectory(prefix='owned engine ') as temp:
            home=Path(temp);lima=home/'fake lima';docker=home/'docker'
            python_command(lima, 'import os,sys,subprocess\nsubprocess.run(sys.argv[sys.argv.index("env"):],env={"PATH":'+repr(str(home)+':/usr/bin:/bin')+'},check=True)\n')
            python_command(docker, 'import os,json,sys\nprint(json.dumps({"arch":os.environ.get("AI_PHONE_CLASH_DESKTOP_DIR"),"cpu":os.environ.get("AI_PHONE_DESKTOP_CPUS"),"args":sys.argv[1:],"secret":os.environ.get("UNRELATED_TEST_SECRET"),"images":{k:os.environ.get(k) for k in ["AI_PHONE_WORKSPACE_IMAGE","AI_PHONE_DESKTOP_IMAGE","AI_PHONE_POSTGRES_IMAGE","AI_PHONE_REDIS_IMAGE","AI_PHONE_IMAGE_PULL_POLICY"]}}))\n')
            lima.chmod(0o755);docker.chmod(0o755)
            folder=write_container_wrapper(home,home,lima)
            images={**{'AI_PHONE_'+role+'_IMAGE':'owned-'+role.lower() for role in ('WORKSPACE','DESKTOP','POSTGRES','REDIS')},'AI_PHONE_IMAGE_PULL_POLICY':'never'}
            result=subprocess.check_output([folder/'docker','compose','path with spaces'],env={**os.environ,**images,'AI_PHONE_CLASH_DESKTOP_DIR':'linux-amd64','AI_PHONE_DESKTOP_CPUS':'4','UNRELATED_TEST_SECRET':'fixture-only'},text=True)
            self.assertEqual(json.loads(result),{'arch':'linux-amd64','cpu':'4','args':['compose','path with spaces'],'secret':None,'images':images})
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
            python_command(manager, 'import sys\nfor text in ["Review licenses (y/N)?", "Fixture license text. Accept? (y/N):"]:\n print(text, end="", flush=True)\n if input() != "y":sys.exit(2)\n')
            manager.chmod(0o755); events=[]
            def emit(phase, state='running', **values): events.append((phase, state, values))
            with patch('sys.stdin', io.StringIO('{"accept":true}\n{"accept":true}\n')):
                sdk_licenses(manager, None, emit, ['platform-tools'])
            licenses=[values['license'] for _,state,values in events if state=='license']
            self.assertEqual(len(licenses),2);self.assertIn('Fixture license text',licenses[1])
            with patch('sys.stdin', io.StringIO('')):
                with self.assertRaisesRegex(ValueError, 'not accepted'): sdk_licenses(manager, None, emit, ['platform-tools'])

    def test_sdk_prompts_cover_only_requested_packages(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);manager=root/'sdkmanager';receipt=root/'args'
            python_command(manager, 'import sys,json\nfrom pathlib import Path\nPath('+repr(str(receipt))+').write_text(json.dumps(sys.argv[1:]))\n')
            manager.chmod(0o755)
            sdk_licenses(manager,None,lambda *_:None,['platform-tools','system-images;android-35;default;x86_64'])
            self.assertEqual(json.loads(receipt.read_text()),['platform-tools','system-images;android-35;default;x86_64'])

    def test_foreign_database_is_refused_before_compose_can_change_anything(self):
        root=Path('/owned')
        def result(owner,argv):
            text='a'*64 if argv[:3]==['docker','container','ls'] else json.dumps({'id':'a'*64,'running':True,'labels':{'com.docker.compose.project':'ai-phone','com.docker.compose.project.working_dir':owner}})
            return subprocess.CompletedProcess([],0,stdout=text)
        verify_container_ownership(root,lambda argv,**_:result('/owned',argv))
        with self.assertRaisesRegex(ValueError, 'Another installation'): verify_container_ownership(root,lambda argv,**_:result('/somebody-else',argv))

    def test_progress_observer_preserves_checkpoint_order_and_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            events=[]
            with InstallState(Path(temp),lambda phase,state:events.append((phase,state))) as install:
                with self.assertRaises(ValueError): install.phase('fixture','input',lambda:None,lambda:False)
                self.assertEqual(install.state['phases']['fixture']['state'],'failed')
            self.assertEqual(events,[('fixture','running'),('fixture','failed')])


if __name__ == '__main__': unittest.main()
