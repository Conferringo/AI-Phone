"""Native Lima compatibility is a compiled artifact property, not edited metadata."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


class NativeLimaTests(unittest.TestCase):
    def fixture(self,root):
        folder=root/'config';folder.mkdir();(folder/'container-host-runtime.json').write_text(json.dumps({'hosts':{'macos-arm64':{'lima':'2.2.1','lima_build':{'source_commit':'a'*40,'source_url':'https://github.com/lima-vm/lima','go_version':'go1.26.9'}}}}))
        config=root/'apps/client/src-tauri/tauri.conf.json';config.parent.mkdir(parents=True);config.write_text(json.dumps({'bundle':{'macOS':{'minimumSystemVersion':'14.4'}}}))

    def test_cache_is_bound_to_source_policy_binary_and_deployment_target(self):
        from lima_native import fingerprint,cached_build
        with tempfile.TemporaryDirectory() as temporary,patch('lima_native.host_key',return_value='macos-arm64'):
            root=Path(temporary);self.fixture(root);folder=root/'.tools/lima-native';folder.mkdir(parents=True);binary=folder/'limactl';binary.write_bytes(b'compiled fixture');binary.chmod(0o755)
            value={'format':1,'fingerprint':fingerprint(root),'source_commit':'a'*40,'go_version':'go1.26.9','minimum_macos':'14.4','file':{'path':'limactl','size':binary.stat().st_size,'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'mode':0o755}}
            receipt=folder/'build.json';receipt.write_text(json.dumps(value));self.assertEqual(cached_build(root)['source_commit'],'a'*40)
            binary.write_bytes(b'changed');self.assertIsNone(cached_build(root));binary.write_bytes(b'compiled fixture')
            receipt.write_text(json.dumps({**value,'minimum_macos':'26.0'}));self.assertIsNone(cached_build(root))
            receipt.write_text(json.dumps({**value,'fingerprint':'0'*64}));self.assertIsNone(cached_build(root))
            for extra in ({'format':True},{'format':0},{'source_commit':'b'*40},{'go_version':'wrong'}):
                receipt.write_text(json.dumps({**value,**extra}));self.assertIsNone(cached_build(root))
            other=folder/'other';other.write_bytes(binary.read_bytes());other.chmod(0o755)
            receipt.write_text(json.dumps({**value,'file':{**value['file'],'path':'other'}}));self.assertIsNone(cached_build(root))
            text=json.dumps(value);receipt.write_text(text+' '*(65536-len(text)));self.assertIsNotNone(cached_build(root));receipt.write_text(receipt.read_text()+' ');self.assertIsNone(cached_build(root))
            for invalid in (None, True, 14.4, '14.not-a-version'):
                receipt.write_text(json.dumps({**value,'minimum_macos':invalid}));self.assertIsNone(cached_build(root))
            receipt.write_text(json.dumps(value));binary.chmod(0o644);self.assertIsNone(cached_build(root))

    def test_compiler_environment_pins_real_link_target_and_local_caches(self):
        from lima_native import build_environment,compile_command
        with tempfile.TemporaryDirectory() as temporary,patch('lima_native.host_key',return_value='macos-arm64'):
            root=Path(temporary);self.fixture(root);env=build_environment(root)
            self.assertEqual(env['MACOSX_DEPLOYMENT_TARGET'],'14.4');self.assertIn('-mmacosx-version-min=14.4',env['CGO_LDFLAGS']);self.assertEqual(env['GOTOOLCHAIN'],'local')
            self.assertTrue(Path(env['GOCACHE']).is_relative_to(root/'.tools'));self.assertEqual(env['GOSUMDB'],'sum.golang.org')
            command=compile_command(root,Path('/compiler/go'),root/'output');self.assertIn('-trimpath',command);self.assertIn('-mod=readonly',command);self.assertIn('-mmacosx-version-min=14.4',' '.join(command))

    @unittest.skipUnless(os.name=='posix','Native Lima producer uses a POSIX executable fixture')
    def test_build_publishes_only_a_version_checked_artifact_and_reuses_it(self):
        from lima_native import build,cached_build
        with tempfile.TemporaryDirectory() as temporary,patch('lima_native.host_key',return_value='macos-arm64'):
            root=Path(temporary);self.fixture(root);(root/'.tools').mkdir();source=root/'source';source.mkdir();go=root/'go';go.write_text('#!/bin/sh\necho go version go1.26.9 darwin/arm64\n');go.chmod(0o755)
            def compile_fixture(actual,src,compiler,output,log):
                output.write_text('#!/bin/sh\necho limactl version 2.2.1\n');output.chmod(0o755)
            actual_run=subprocess.run
            def run(args,**kwargs):
                return subprocess.CompletedProcess(args,0) if args[0]=='codesign' else actual_run(args,**kwargs)
            with patch('lima_native.source_identity',return_value='a'*40),patch('lima_native.compile_lima',side_effect=compile_fixture) as compile_run,patch('lima_native.subprocess.run',side_effect=run),patch('lima_native.verify_targets',return_value={'lima':'14.4'}):
                first=build(root,source,go);self.assertEqual(first,cached_build(root));self.assertEqual(build(root,source,go),first);self.assertEqual(compile_run.call_count,1)
                (root/'.tools/lima-native/limactl').write_text('broken')
                with self.assertRaisesRegex(ValueError,'Stale'):build(root,source,go)

    @unittest.skipUnless(os.name=='posix','Process-group cancellation targets the POSIX Lima factory')
    def test_real_compiler_child_is_reaped_on_disk_guard_and_cancellation(self):
        from lima_native import compile_lima
        with tempfile.TemporaryDirectory() as temporary,patch('lima_native.host_key',return_value='macos-arm64'):
            root=Path(temporary);self.fixture(root);children=[];actual=subprocess.Popen
            def spawn(*args,**kwargs):
                child=actual(*args,**kwargs);children.append(child);return child
            with patch('lima_native.compile_command',return_value=[sys.executable,'-c','import time;time.sleep(60)']),patch('lima_native.subprocess.Popen',side_effect=spawn),patch('lima_native.shutil.disk_usage',return_value=type('Space',(),{'free':0})()):
                with self.assertRaisesRegex(ValueError,'4 GiB'):compile_lima(root,root,Path(sys.executable),root/'output',root/'compile.log')
            self.assertIsNotNone(children[0].poll())
            with patch('lima_native.compile_command',return_value=[sys.executable,'-c','import time;time.sleep(.05)']),patch.dict(os.environ,{'AI_PHONE_BOOTSTRAP_PROXY':'http://127.0.0.1:9'}):
                compile_lima(root,root,None,None,root/'success.log')
            with patch('lima_native.compile_command',return_value=[sys.executable,'-c','raise SystemExit(7)']):
                with self.assertRaisesRegex(ValueError,'compilation failed'):compile_lima(root,root,None,None,root/'failure.log')
            ready=root/'child.pid';script=root/'cancel.py';scripts=Path(__file__).resolve().parent
            script.write_text('import sys,signal\nsys.path.insert(0,'+repr(str(scripts))+')\nimport lima_native\nfrom pathlib import Path\nsignal.signal(signal.SIGTERM,lima_native.cancelled)\nlima_native.build_environment=lambda root:'+repr({key:str(root/key) for key in ('GOCACHE','GOMODCACHE','GOTMPDIR')})+'\nlima_native.compile_command=lambda *args:'+repr([sys.executable,'-c',f'import os,time;from pathlib import Path;Path({str(ready)!r}).write_text(str(os.getpid()));time.sleep(60)'])+'\nlima_native.compile_lima(Path('+repr(str(root))+'),Path('+repr(str(root))+'),None,None,Path('+repr(str(root/'cancel.log'))+'))\n')
            with (root/'cancel-driver.log').open('w') as log:
                driver=actual([sys.executable,str(script)],stdout=log,stderr=subprocess.STDOUT)
                try:
                    deadline=time.monotonic()+5
                    while not ready.exists() and driver.poll() is None and time.monotonic()<deadline:time.sleep(.01)
                    self.assertTrue(ready.is_file(),(root/'cancel-driver.log').read_text());pid=int(ready.read_text());driver.send_signal(signal.SIGTERM);self.assertNotEqual(driver.wait(timeout=5),0)
                    with self.assertRaises(ProcessLookupError):os.kill(pid,0)
                finally:
                    if driver.poll() is None:driver.kill();driver.wait()

    def test_source_identity_rejects_a_different_commit_and_unreviewed_files(self):
        from lima_native import source_identity
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);subprocess.run(['git','init','-q'],cwd=root,check=True);subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@aiphone.invalid','commit','--allow-empty','-m','isolated fixture','-q'],cwd=root,check=True)
            identity=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();self.assertEqual(source_identity(root,identity),identity)
            with self.assertRaises(ValueError):source_identity(root,'0'*40)
            (root/'unreviewed').write_text('fixture')
            with self.assertRaises(ValueError):source_identity(root,identity)


if __name__=='__main__':unittest.main()
