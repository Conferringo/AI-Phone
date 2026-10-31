"""Real tar reader budgets and WSL process environment boundaries."""
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from test_wsl_backend import rootfs
import wsl_backend
import wsl_backend_entry as entry


class BudgetedHeaders(io.BytesIO):
    def __init__(self):
        def header(name,size=0):
            row=tarfile.TarInfo(name);row.size=size;return row.tobuf()
        prefix=header(wsl_backend.BODY,1)+b'x'.ljust(512,b'\0')+header(wsl_backend.MANIFEST,1)+b'x'.ljust(512,b'\0')
        self.budget=2*1024+512
        super().__init__(prefix+header('unexpected-third')+header('unread')*10000+b'\0'*1024)
        self.maximum_read=0;self.bytes_read=0
    def read(self,size=-1):
        end=self.tell()+(size if size>=0 else len(self.getvalue())-self.tell())
        if end>self.budget:raise AssertionError('Reader scanned beyond the third outer header')
        result=super().read(size);self.maximum_read=max(self.maximum_read,self.tell());self.bytes_read+=len(result);return result


class WslArchiveBudgetTests(unittest.TestCase):
    def test_rejects_third_header_without_reading_remaining_outer_archive(self):
        source=BudgetedHeaders()
        with self.assertRaisesRegex(ValueError,'members'):wsl_backend.verify_stream(source)
        self.assertEqual(source.maximum_read,source.budget)
        self.assertEqual(source.bytes_read,3*512)

    def test_pax_header_is_rejected_before_reading_its_declared_body(self):
        header=tarfile.TarInfo('pax');header.type=tarfile.XHDTYPE;header.size=2**40
        raw=io.BytesIO(header.tobuf())
        with self.assertRaisesRegex(ValueError,'members'):wsl_backend.verify_stream(raw)
        self.assertEqual(raw.tell(),512)

    def test_valid_capsule_never_looks_up_members_by_string(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);body=root/'rootfs.tar.gz';rootfs(body)
            payload=root/'payload';wsl_backend.publish_capsule(body,payload,{})
            with patch.object(tarfile.TarFile,'getmembers',side_effect=AssertionError('Unbounded enumeration')),patch.object(tarfile.TarFile,'getmember',side_effect=AssertionError('String member lookup')):
                self.assertFalse(wsl_backend.verify_capsule(payload)['complete_device_bundle'])


class WslRuntimeBoundaryTests(unittest.TestCase):
    def test_environment_owns_socket_and_does_not_inherit_foreign_docker_context(self):
        with patch.dict(os.environ,{'DOCKER_HOST':'tcp://foreign:2375','DOCKER_CONTEXT':'foreign','DOCKER_TLS':'1','DOCKER_TLS_VERIFY':'1','DOCKER_CERT_PATH':'/foreign','DOCKER_CONFIG':'/foreign','LIMA_HOME':'/foreign','AI_PHONE_MANAGED_BIN':'/foreign'},clear=True):
            value=entry.environment()
            self.assertEqual(value['DOCKER_HOST'],entry.SOCKET)
            self.assertNotEqual(value['DOCKER_CONFIG'],'/foreign')
            for name in ('DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH','LIMA_HOME','AI_PHONE_MANAGED_BIN'):
                self.assertNotIn(name,value)

    def test_real_web_helper_gets_final_port_and_does_not_start_lan_or_phone(self):
        captured=[]
        def capture(root,name,argv,environment):captured.append((name,list(map(str,argv)),dict(environment)))
        with tempfile.TemporaryDirectory() as folder,patch.dict(os.environ,{},clear=False):
            root=Path(folder);(root/'runtime').mkdir();(root/'runtime/manifest.json').write_text('{}')
            with patch('client_host.verify_container_ownership',return_value={}),patch('container_images.stage'),patch('container_images.activate',return_value={}),patch('linux_services.unit_state',return_value=None),patch('linux_services.start_unit',side_effect=capture),patch('prebuilt_services.launch_spec',return_value={'argv':['assistant'],'environment':{}}),patch('prebuilt_js.launch_spec',side_effect=lambda _root,name:{'argv':[name],'environment':{}}),patch('prebuilt_web.web_command',return_value=['/fixture/bun','/fixture/server.js']),patch('linux_host.shutil.which',return_value='/fixture/node'):
                # runtime_env and web startup helper remain real: the regression covers their composition.
                entry.start_backend(root,lambda *_args,**_kwargs:SimpleNamespace(returncode=0))
            names=[name for name,_,_ in captured]
            self.assertEqual(names,['assistant','web','channels','media'])
            web=next(env for name,_,env in captured if name=='web')
            self.assertEqual(web['PORT'],'17860');self.assertEqual(web['HOSTNAME'],'127.0.0.1')
            self.assertEqual(web['DOCKER_HOST'],entry.SOCKET)


class WslOwnershipChainTests(unittest.TestCase):
    def image_payload(self,folder):
        import hashlib
        from test_wsl_backend import archive
        from wsl_backend import add_bytes
        images={};files=[];docker_manifest=[]
        for role in ('workspace','desktop','postgres','redis'):
            data=json.dumps({'os':'linux','architecture':'amd64','role':role}).encode()
            identity=hashlib.sha256(data).hexdigest();name=identity+'.json';reference='ai-phone-bundle/'+role+':sha-'+identity
            images[role]={'source_digest':'sha256:'+identity,'export_digest':'sha256:'+identity,'config_digest':'sha256:'+identity,'ref':reference,'size':len(data)}
            files.append((name,data));docker_manifest.append({'Config':name,'RepoTags':[reference],'Layers':[]})
        inner=folder/'images.tar.gz';archive(inner,[*files,('manifest.json',json.dumps(docker_manifest).encode())]);body=inner.read_bytes()
        manifest={'format':1,'os':'linux','architecture':'amd64','redistribution':'review-pending','images':images,'archive':{'path':'images.tar.gz','size':len(body),'sha256':hashlib.sha256(body).hexdigest()}}
        payload=folder/'images.payload'
        with tarfile.open(payload,'w:') as target:
            add_bytes(target,'images.tar.gz',body);add_bytes(target,'IMAGES-MANIFEST.json',json.dumps(manifest).encode())
        return manifest

    def test_real_image_activation_profile_ownership_and_web_chain_share_one_socket(self):
        import managed_engine
        captures=[];docker_calls=[]
        with tempfile.TemporaryDirectory() as folder,patch.dict(os.environ,{'DOCKER_CONTEXT':'wrong','LIMA_HOME':'wrong','AI_PHONE_MANAGED_BIN':'/wrong'}):
            root=Path(folder);bundle=root/'bundle';bundle.mkdir();manifest=self.image_payload(bundle)
            (root/'runtime').mkdir();(root/'runtime/manifest.json').write_text('{}')
            def docker(argv,**kwargs):
                docker_calls.append(list(map(str,argv)))
                self.assertEqual(os.environ['DOCKER_HOST'],entry.SOCKET);self.assertNotIn('DOCKER_CONTEXT',os.environ)
                if argv[1:3]==['container','ls']:text='a'*64
                elif argv[1:3]==['container','inspect']:
                    text=json.dumps({'id':'a'*64,'running':True,'labels':{'com.docker.compose.project':'ai-phone','com.docker.compose.project.working_dir':str(root)}})
                elif argv[1]=='info':text='amd64'
                elif argv[1:3]==['image','inspect']:
                    image=next(row for row in manifest['images'].values() if row['ref']==argv[3])
                    text=json.dumps([{'Id':image['export_digest'],'Architecture':'amd64','Os':'linux'}])
                else:self.fail('Unexpected external command: '+str(argv))
                return SimpleNamespace(returncode=0,stdout=text)
            def start(root,name,argv,env):captures.append((name,dict(env)))
            with patch.object(entry,'BUNDLE',bundle),patch('subprocess.run',side_effect=docker),patch('managed_engine.environment',wraps=managed_engine.environment) as managed,patch('linux_services.start_unit',side_effect=start),patch('linux_services.unit_state',return_value=None),patch('prebuilt_services.launch_spec',return_value={'argv':['assistant'],'environment':{}}),patch('prebuilt_js.launch_spec',side_effect=lambda _root,name:{'argv':[name],'environment':{}}),patch('prebuilt_web.web_command',return_value=['/fixture/bun','/fixture/web']),patch('linux_host.shutil.which',return_value='/fixture/node'):
                entry.start_backend(root,lambda *_args,**_kwargs:SimpleNamespace(returncode=0))
            managed.assert_called_once_with(root)
            self.assertEqual(len(docker_calls),15)
            self.assertEqual([name for name,_ in captures],['assistant','web','channels','media'])
            for _,env in captures:
                self.assertEqual(env['DOCKER_HOST'],entry.SOCKET);self.assertNotIn('LIMA_HOME',env)
                self.assertNotIn('/wrong',env['PATH'])
            self.assertEqual(next(env for name,env in captures if name=='web')['PORT'],'17860')
            recorded=json.loads((root/'data/deployment/image-runtime.json').read_text())
            self.assertEqual(recorded,manifest)

    def test_lima_owned_state_is_rejected_before_queries_or_environment_changes(self):
        with tempfile.TemporaryDirectory() as folder,patch.dict(os.environ,{'DOCKER_CONTEXT':'original'}):
            root=Path(folder);record=root/'data/deployment/container-host.json';record.parent.mkdir(parents=True);record.write_bytes(b'preserved')
            with patch('client_host.verify_container_ownership',return_value={}) as query:
                with self.assertRaisesRegex(ValueError,'Lima-owned'):entry.start_backend(root,lambda *_args:None)
            query.assert_not_called();self.assertEqual(record.read_bytes(),b'preserved')
            self.assertEqual(os.environ['DOCKER_CONTEXT'],'original')


if __name__=='__main__':unittest.main()
