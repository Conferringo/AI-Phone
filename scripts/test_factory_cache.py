"""Disposable build intermediates retire only after their native payload is sealed."""
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from artifact_io import digest
from factory_cache import retire_native_cache
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

if __name__=='__main__':unittest.main()
