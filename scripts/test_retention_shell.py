"""Exercise the actual retention shell against a local GitHub fixture."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


DRIVER = r'''
import json,os,re,runpy,subprocess,sys
from pathlib import Path
tool,*args=sys.argv[1:]
root=Path(os.environ['RETENTION_FIXTURE'])
def emit(value):sys.stdout.buffer.write(json.dumps(value).encode()+b'\n')
if tool=='git':
    sys.stdout.buffer.write(os.environ['GITHUB_SHA'].encode()+b'\n')
elif tool=='jq':
    result=subprocess.run([os.environ['RETENTION_JQ'],*args],capture_output=True)
    binary=any(arg=='--binary' or arg.startswith('-') and not arg.startswith('--') and 'b' in arg for arg in args)
    output=result.stdout
    if os.environ['RETENTION_CRLF']!='none' and not binary:output=output.replace(b'\n',b'\r\n')
    sys.stdout.buffer.write(output);sys.stderr.buffer.write(result.stderr);sys.exit(result.returncode)
elif tool=='python3':
    if args[0]=='-c':
        result=subprocess.run([sys.executable,*args],capture_output=True)
        output=result.stdout
        if os.environ['RETENTION_CRLF']=='all':output=output.replace(b'\n',b'\r\n')
        sys.stdout.buffer.write(output);sys.stderr.buffer.write(result.stderr);sys.exit(result.returncode)
    sys.argv=args;runpy.run_path(args[0],run_name='__main__')
elif tool=='gh':
    if args[0]=='release':
        assert args[:2]==['release','upload'],args
        request=json.loads((root/'request.json').read_text())
        assert args[2]==request['tag_name']
        files=args[5:]
        assert len(files)==2,files
        assert [Path(name).resolve() for name in files]==[root/'dist/complete.exe',root/'dist/complete.exe.manifest.json'],files
        assert all(Path(name).is_file() for name in files),files
        (root/'uploaded.json').write_text(json.dumps(files))
    elif '--method' in args:
        assert args[args.index('--method')+1]=='POST',args
        value=json.loads(Path(args[args.index('--input')+1]).read_text())
        assert re.fullmatch(r'candidate-[a-f0-9]{12}-123-[a-f0-9]{32}',value['tag_name']),repr(value['tag_name'])
        assert value['target_commitish']==os.environ['GITHUB_SHA']
        (root/'request.json').write_text(json.dumps(value));emit({**value,'id':47,'published_at':None})
    elif 'git/ref/tags/' in args[-1]:
        sys.stderr.write('HTTP 404\n');sys.exit(1)
    elif args[-1].endswith('releases?per_page=100'):emit([[]])
    elif args[-1].endswith('/47'):
        emit({**json.loads((root/'request.json').read_text()),'id':47,'published_at':None})
    elif args[-1].endswith('/47/assets?per_page=100'):emit([[]])
    else:raise AssertionError(args)
else:raise AssertionError(tool)
'''


class RetentionShellTests(unittest.TestCase):
    def test_local_upload_preserves_paths_ids_and_tags_with_native_windows_stdout(self):
        bash=shutil.which('bash');jq=shutil.which('jq')
        self.assertIsNotNone(bash,'Retention requires Bash')
        self.assertIsNotNone(jq,'Retention requires jq')
        script=Path(__file__).with_name('retain-client-candidate.sh').resolve()
        for newline in ('none','jq','all'):
            with self.subTest(stdout=newline),tempfile.TemporaryDirectory(prefix='retention fixture ') as temporary:
                root=Path(temporary).resolve()
                for name in ('bin','scripts','dist','temp'):(root/name).mkdir()
                driver=root/'driver.py';driver.write_text(DRIVER)
                for tool in ('git','jq','python3','gh'):
                    wrapper=root/'bin'/tool
                    wrapper.write_bytes(('#!/usr/bin/env bash\nexec "$RETENTION_PYTHON" "$RETENTION_DRIVER" '+tool+' "$@"\n').encode())
                    wrapper.chmod(0o755)
                for name in ('complete.exe','complete.exe.manifest.json'):(root/'dist'/name).write_bytes(b'fixture')
                (root/'scripts/check_source.py').write_text('print("{}")\n')
                (root/'scripts/package_delivery.py').write_text(
                    'import json,sys\nfrom pathlib import Path\n'
                    'if sys.argv[1]=="plan":print(json.dumps({"files":[{"path":"complete.exe"},{"path":"complete.exe.manifest.json"}]}))\n'
                    'else:\n assert Path("uploaded.json").is_file()\n Path("verified").touch()\n')
                environment={**os.environ,'PATH':str(root/'bin')+os.pathsep+os.environ['PATH'],
                             'GITHUB_ACTIONS':'true','RUNNER_ENVIRONMENT':'github-hosted',
                             'GITHUB_SHA':'a'*40,'GITHUB_RUN_ID':'123','GITHUB_RUN_ATTEMPT':'1',
                             'GITHUB_REPOSITORY':'fixture/fixture','RUNNER_TEMP':str(root/'temp'),
                             'GITHUB_OUTPUT':str(root/'outputs'),'RETENTION_FIXTURE':str(root),
                             'RETENTION_DRIVER':str(driver),'RETENTION_PYTHON':sys.executable,
                             'RETENTION_JQ':jq,'RETENTION_CRLF':newline}
                result=subprocess.run([bash,str(script),'dist/report.json'],cwd=root,env=environment,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                self.assertTrue((root/'verified').is_file())
                self.assertIn('candidate_release_id=47\n',(root/'outputs').read_text())
                self.assertEqual(list((root/'temp').iterdir()),[])


if __name__=='__main__':unittest.main()
