"""Real sealed JS control consumer; WSL startup ordering is covered by Rust."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import prebuilt_js
import test_prebuilt_js as runtime_fixture

HERE = Path(__file__).resolve().parent
SOURCE = Path(os.environ.get('AI_PHONE_TEST_SOURCE_ROOT', str(HERE.parent))).resolve()
BUN = Path(os.environ.get('AI_PHONE_TEST_BUN', str(SOURCE/'.tools/bin/bun'))).resolve()
DEVICE = '00000000-0000-4000-8000-000000000042'


def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


@unittest.skipUnless(os.name=='posix','sealed Bun CLI consumer runs on Linux/macOS')
class SealedControlConsumerTests(unittest.TestCase):
    def fixture(self, root):
        value=runtime_fixture.JavaScriptRuntimeTests().fixture(root)
        from test_native_services import NativeServiceTests
        from test_release import native_profile_program
        from prebuilt_services import binary_path, RUNTIME_ENVIRONMENT_CONTRACT
        services=NativeServiceTests().component()
        services[binary_path('assistant')]=Path(native_profile_program()).read_bytes()
        manifest=json.loads(services['runtime/services/manifest.json'])
        manifest['runtime_environment']=RUNTIME_ENVIRONMENT_CONTRACT
        for row in manifest['files']:
            row.update(size=len(services[row['path']]),sha256=hashlib.sha256(services[row['path']]).hexdigest())
        services['runtime/services/manifest.json']=json.dumps(manifest).encode()
        for name,data in services.items():
            path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data);path.chmod(0o755 if '/bin/' in name else 0o644)
        for name in ('control.ts','config.ts','native-apps.ts','app-assets.ts'):
            path=root/'apps/device-network'/name;path.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(SOURCE/'apps/device-network'/name,path)
        launcher=root/'scripts/prebuilt_js.py';launcher.parent.mkdir(exist_ok=True)
        shutil.copyfile(HERE/'prebuilt_js.py',launcher)
        shutil.copyfile(HERE/'runtime_config.py',launcher.parent/'runtime_config.py')
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
            cache=root/'.tools'
            self.assertEqual(sorted(path.relative_to(cache).as_posix() for path in cache.rglob('*')),
                             ['mihomo','mihomo/linux-arm64'])
            self.assertTrue(all(path.is_dir() for path in cache.rglob('*')))
            self.assertFalse((root/'node_modules').exists())
        self.assertEqual(digest(BUN),original)


if __name__=='__main__':unittest.main()
