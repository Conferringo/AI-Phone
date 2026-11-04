"""Disposable build intermediates retire only after their native payload is sealed."""
import hashlib
import io
import json
import os
import socket
import subprocess
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from artifact_io import digest
from factory_cache import retire_native_cache
import factory_cache
from prebuilt_services import binary_path,DEVICE_SETUP_CONTRACT

class FactoryCacheTests(unittest.TestCase):
    def fixture(self,root):
        folder=root/'target/release';folder.mkdir(parents=True)
        artifacts={};files=[]
        for role in ['assistant','runner','vision']:
            path='target/release/ai-phone-'+role;body=(role+' fixture').encode();target=root/path;target.write_bytes(body);target.chmod(0o755)
            record={'path':path,'size':len(body),'sha256':hashlib.sha256(body).hexdigest()};artifacts[role]=record
            files.append((binary_path(role),body))
        files.append(('runtime/services/manifest.json',b'{}'))
        record={'format':1,'checks':{'device_setup':DEVICE_SETUP_CONTRACT},'artifacts':artifacts}
        (folder/'native-service-build.json').write_text(json.dumps(record))
        payload=root/'apps/client/src-tauri/gen/host/source.payload';payload.parent.mkdir(parents=True)
        manifest={'format':2,'components':['native-services'],'files':[{'path':name,'size':len(body),'sha256':hashlib.sha256(body).hexdigest(),'mode':0o755 if '/bin/' in name else 0o644} for name,body in files]}
        files.append(('HOST-MANIFEST.json',json.dumps(manifest).encode()))
        with tarfile.open(payload,'w:xz') as archive:
            for name,body in files:
                entry=tarfile.TarInfo(name);entry.size=len(body);archive.addfile(entry,io.BytesIO(body))
        payload.with_name('integrity.json').write_text(json.dumps({'source.payload':digest(payload)}))
        for profile in ['release','debug']:
            for name in ['deps','build','.fingerprint','incremental']:
                directory=root/'target'/profile/name;directory.mkdir(parents=True);(directory/'fixture').write_bytes(b'cache'*100)
        state=root/'user/chat';state.parent.mkdir();state.write_text('keep')
        return payload

    def environment(self,root):
        return patch.dict(os.environ,{'GITHUB_ACTIONS':'true','RUNNER_ENVIRONMENT':'github-hosted','RUNNER_OS':'Linux','AI_PHONE_DISPOSABLE_QUALIFICATION':'1','GITHUB_WORKSPACE':str(root)},clear=True)

    def test_retire_caches_preserves_sealed_payload_programs_and_user_files(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload=self.fixture(root);original=digest(payload)
            with self.environment(root):report=retire_native_cache(root,payload)
            self.assertEqual(len(report['retired']),8)
            self.assertEqual(digest(payload),original)
            self.assertEqual((root/'user/chat').read_text(),'keep')
            self.assertEqual((root/'target/release/ai-phone-assistant').read_bytes(),b'assistant fixture')
            self.assertFalse((root/'target/release/deps').exists())

    def test_local_runs_changed_payload_and_escaped_cache_never_delete(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload=self.fixture(root)
            with patch.dict(os.environ,{},clear=True),self.assertRaises(ValueError):retire_native_cache(root,payload)
            with self.environment(root):
                os.environ.pop('GITHUB_WORKSPACE')
                with self.assertRaises(ValueError):retire_native_cache(root,payload)
            self.assertTrue((root/'target/release/deps/fixture').is_file())
            with self.environment(root):
                (root/'target/release/ai-phone-assistant').write_bytes(b'changed')
                with self.assertRaises(ValueError):retire_native_cache(root,payload)
            self.assertTrue((root/'target/release/deps/fixture').is_file())
        with tempfile.TemporaryDirectory() as temp,tempfile.TemporaryDirectory() as outside:
            root=Path(temp);payload=self.fixture(root);cache=root/'target/release/deps'
            import shutil
            shutil.rmtree(cache);Path(outside,'keep').write_text('keep');cache.symlink_to(outside,target_is_directory=True)
            with self.environment(root),self.assertRaises(ValueError):retire_native_cache(root,payload)
            self.assertEqual(Path(outside,'keep').read_text(),'keep')
            self.assertTrue((root/'target/debug/deps/fixture').is_file())

    def test_image_cache_retirement_targets_only_the_disposable_action_socket(self):
        with tempfile.TemporaryDirectory(prefix='fc-',dir='/tmp') as temp:
            root=Path(temp).resolve();payload=root/'images.payload';payload.write_bytes(b'sealed image fixture')
            endpoint=root/'setup-docker-action/run-1234abcd/docker.sock';endpoint.parent.mkdir(parents=True)
            with socket.socket(socket.AF_UNIX) as owned:
                owned.bind(str(endpoint))
                with self.environment(root),patch.dict(os.environ,{'DOCKER_HOST':'unix://'+str(endpoint),'DOCKER_CONTEXT':'foreign','DOCKER_TLS_VERIFY':'1'}), \
                     patch.object(Path,'home',return_value=root),patch.object(factory_cache.subprocess,'run') as run:
                    with patch.dict(os.environ,{'GITHUB_WORKSPACE':str(root/'other')}),self.assertRaises(ValueError):
                        factory_cache.retire_image_build_cache(root,payload)
                    run.assert_not_called()
                    result=factory_cache.retire_image_build_cache(root,payload)
                    self.assertEqual(run.call_args.args[0],['docker','--host','unix://'+str(endpoint),'builder','prune','--all','--force'])
                    self.assertNotIn('DOCKER_CONTEXT',run.call_args.kwargs['env'])
                    self.assertNotIn('DOCKER_TLS_VERIFY',run.call_args.kwargs['env'])
                    run.side_effect=subprocess.CalledProcessError(1,['docker'])
                    with self.assertRaises(subprocess.CalledProcessError):factory_cache.retire_image_build_cache(root,payload)
                    run.side_effect=lambda *a,**k:payload.write_bytes(b'changed')
                    with self.assertRaisesRegex(ValueError,'changed'):factory_cache.retire_image_build_cache(root,payload)
                    payload.write_bytes(b'sealed image fixture')
                self.assertEqual(payload.read_bytes(),b'sealed image fixture')
                self.assertFalse(result['capacity_guard_changed']);self.assertFalse(result['docker_images_removed'])

    def test_local_or_foreign_engine_never_reaches_cache_pruning(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp).resolve();payload=root/'images.payload';payload.write_bytes(b'keep')
            with patch.object(factory_cache.subprocess,'run') as run:
                with patch.dict(os.environ,{},clear=True),self.assertRaises(ValueError):
                    factory_cache.retire_image_build_cache(root,payload)
                for endpoint in ['tcp://remote:2375','unix:///var/run/docker.sock','unix://'+str(root/'setup-docker-action/run-1234abcd/missing.sock')]:
                    with self.environment(root),patch.dict(os.environ,{'DOCKER_HOST':endpoint}), \
                         patch.object(Path,'home',return_value=root),self.assertRaises(ValueError):
                        factory_cache.retire_image_build_cache(root,payload)
                run.assert_not_called()

if __name__=='__main__':unittest.main()
