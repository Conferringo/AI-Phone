"""WSL startup/provision ordering and the real sealed JS control consumer."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import client_host
import prebuilt_js
import wsl_backend_entry as entry
import test_prebuilt_js as runtime_fixture

HERE = Path(__file__).resolve().parent
SOURCE = Path(os.environ.get('AI_PHONE_TEST_SOURCE_ROOT', str(HERE.parent))).resolve()
BUN = Path(os.environ.get('AI_PHONE_TEST_BUN', str(SOURCE/'.tools/bin/bun'))).resolve()
DEVICE = '00000000-0000-4000-8000-000000000042'


def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


def observed(root, name, running=False):
    return {'id':hashlib.sha256(name.encode()).hexdigest(), 'running':running,
            'labels':{'com.docker.compose.project':'ai-phone','com.docker.compose.project.working_dir':str(root)}}


def response(value=None, error=None):
    return subprocess.CompletedProcess([],1 if error else 0,stdout=json.dumps(value) if value else '',stderr=error or '')


def container_query(name, inspection):
    def query(argv, **_):
        if argv[:3]==['docker','container','ls']:
            identifier=hashlib.sha256(name.encode()).hexdigest() if argv[5]=='name=^/'+name+'$' else ''
            return subprocess.CompletedProcess([],0,stdout=identifier,stderr='')
        return inspection
    return query


class ProvisionOrderingTests(unittest.TestCase):
    def run_backend(self, root, query):
        commands=[];started=[];images=[]
        self.effects={'commands':commands,'started':started,'images':images}
        def execute(argv, **_):
            commands.append(list(map(str,argv)))
            # Docker's bind-directory effect, without starting Docker or a guest.
            if 'compose.desktop.yaml' in argv:
                (root/'data/device-network/desktop').mkdir(parents=True,exist_ok=True)
            return response()
        with ExitStack() as stack:
            stack.enter_context(patch.dict(os.environ,dict(os.environ)))
            stack.enter_context(patch('client_host.verify_container_ownership',side_effect=lambda selected:client_host_identity(selected,query)))
            stack.enter_context(patch('container_images.stage',side_effect=lambda *_:images.append('stage')))
            stack.enter_context(patch('container_images.activate',return_value={}))
            stack.enter_context(patch('runtime_config.runtime_env',return_value={}))
            stack.enter_context(patch('linux_host.private_token'))
            stack.enter_context(patch('linux_host.start_web_service',side_effect=lambda _r,_e,_run,start:start('web',['web'])))
            stack.enter_context(patch('linux_services.start_unit',side_effect=lambda _r,name,argv,env:started.append(name)))
            stack.enter_context(patch('linux_services.unit_state',return_value=None))
            stack.enter_context(patch('prebuilt_services.launch_spec',return_value={'argv':['assistant'],'environment':{}}))
            stack.enter_context(patch('prebuilt_js.launch_spec',side_effect=lambda _r,name:{'argv':[name],'environment':{}}))
            entry.start_backend(root,execute)
        return commands,started

    def test_fresh_backend_does_not_create_desktop_before_api_provision(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            calls,names=self.run_backend(root,lambda *_,**__:response())
            self.assertFalse((root/'data/device-network/desktop').exists())
            self.assertFalse(any('compose.desktop.yaml' in call or call[:2]==['docker','start'] for call in calls))
            self.assertTrue(any(call[-2:]==['postgres','redis'] and '--no-build' in call and '--pull' in call and 'never' in call for call in calls))
            self.assertEqual(names,['assistant','web','channels','media'])

    def test_existing_desktop_only_resumes_exact_verified_id_without_recreation(self):
        for running in (False,True):
            with self.subTest(running=running),tempfile.TemporaryDirectory() as folder:
                root=Path(folder);identity=observed(root,'ai-phone-desktop',running)
                marker=root/'chat';marker.write_text('keep')
                calls,names=self.run_backend(root,container_query('ai-phone-desktop',response(identity)))
                starts=[call for call in calls if call[:2]==['docker','start']]
                self.assertEqual(starts,[] if running else [['docker','start',identity['id']]])
                self.assertFalse(any('compose.desktop.yaml' in call or '--cpus' in call or '--memory' in call for call in calls))
                self.assertEqual(marker.read_text(),'keep');self.assertEqual(names,['assistant','web','channels','media'])

    def test_foreign_or_unknown_ownership_fails_before_any_backend_effect(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);foreign=observed(Path('/foreign'),'ai-phone-desktop')
            malformed=observed(root,'ai-phone-desktop');malformed['running']='false'
            unknown_id=observed(root,'ai-phone-desktop');unknown_id['id']=int('1'*64)
            for value in (response(foreign),response(malformed),response(unknown_id),response(error='Cannot connect to daemon')):
                with self.subTest(value=value):
                    with self.assertRaises(ValueError):self.run_backend(root,container_query('ai-phone-desktop',value))
                    self.assertEqual(self.effects,{'commands':[],'started':[],'images':[]})
                    self.assertFalse((root/'data').exists())

    def test_failed_container_listing_leaves_backend_and_data_untouched(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            query=lambda *_,**__:response(error='Error: No such object: ai-phone-postgres-1')
            with self.assertRaisesRegex(ValueError,'could not be verified'):self.run_backend(root,query)
            self.assertEqual(self.effects,{'commands':[],'started':[],'images':[]})
            self.assertFalse((root/'data').exists())


client_host_identity=client_host.verify_container_ownership


@unittest.skipUnless(os.name=='posix','sealed Bun CLI consumer runs on Linux/macOS')
class SealedControlConsumerTests(unittest.TestCase):
    def fixture(self, root):
        value=runtime_fixture.JavaScriptRuntimeTests().fixture(root)
        for name in ('control.ts','config.ts','native-apps.ts','app-assets.ts'):
            path=root/'apps/device-network'/name;path.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(SOURCE/'apps/device-network'/name,path)
        launcher=root/'scripts/prebuilt_js.py';launcher.parent.mkdir(exist_ok=True)
        shutil.copyfile(HERE/'prebuilt_js.py',launcher)
        binary=root/'runtime/bin/bun';binary.unlink();os.link(BUN,binary)
        control=root/prebuilt_js.PREFIX/'apps/device-network/control.js'
        control.parent.mkdir(parents=True,exist_ok=True)
        subprocess.run([str(binary),'--no-install','build','apps/device-network/control.ts','--target=bun','--packages=external','--outfile='+str(control)],cwd=root,check=True,capture_output=True)
        control.chmod(0o644)
        records={row['path']:row for row in value['files']}
        for path in (control,):
            name=path.relative_to(root).as_posix();records[name]={'path':name,'size':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'mode':0o644}
        value['files']=list(records.values());value['source_sha256']=prebuilt_js.source_fingerprint(root)
        (root/prebuilt_js.MANIFEST).write_text(json.dumps(value))
        web=json.loads((root/'runtime/manifest.json').read_text())
        for row in web['files']:
            if row['path']=='runtime/bin/bun':row.update(size=binary.stat().st_size,sha256=digest(binary))
        (root/'runtime/manifest.json').write_text(json.dumps(web))
        return launcher

    def test_sealed_control_cli_prepares_alias_without_private_bun_or_development_modules(self):
        original=digest(BUN)
        with tempfile.TemporaryDirectory(prefix='sealed-control-',dir=HERE.parent) as folder:
            root=Path(folder).resolve();launcher=self.fixture(root)
            self.assertFalse((root/'.tools').exists());self.assertFalse((root/'node_modules').exists())
            environment={**os.environ,'PYTHONPATH':str(HERE)+os.pathsep+str(SOURCE/'scripts'),'PYTHONDONTWRITEBYTECODE':'1'}
            request=json.dumps({'operation':'prepare','kind':'desktop','deviceId':DEVICE})
            command=[sys.executable,str(launcher),'--root',str(root),'--role','device-network','--run']
            for _ in range(2):
                result=subprocess.run(command,input=request,text=True,capture_output=True,env=environment,timeout=10)
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertEqual(json.loads(result.stdout),{'prepared':True})
            alias=root/'data/device-network/desktop';self.assertTrue(alias.is_symlink())
            self.assertEqual(alias.resolve(),root/'data/device-network'/DEVICE)
            # The original eager Compose startup would have created this directory.
            # The real control consumer must refuse it, not adopt/delete foreign state.
            alias.unlink();alias.mkdir();(alias/'keep').write_text('existing')
            blocked=subprocess.run(command,input=request,text=True,capture_output=True,env=environment,timeout=10)
            self.assertNotEqual(blocked.returncode,0);self.assertTrue(json.loads(blocked.stdout)['error'])
            self.assertEqual((alias/'keep').read_text(),'existing');self.assertFalse(alias.is_symlink())
            self.assertFalse((root/'.tools').exists());self.assertFalse((root/'node_modules').exists())
        self.assertEqual(digest(BUN),original)


if __name__=='__main__':unittest.main()
