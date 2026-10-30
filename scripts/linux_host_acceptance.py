"""Fresh isolated CI deployment through the same worker used by the native client.

Only the CI image's already accepted SDK license files are copied. No SDK terms
are accepted by this script, and no model/account/chat fixtures are imported.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from prepare_client_host import prepare
from client_host_entry import stage_source
from client_host import download


def main():
    if os.environ.get('GITHUB_ACTIONS')!='true' or os.environ.get('RUNNER_OS')!='Linux':raise ValueError('Fresh-host acceptance is restricted to an ephemeral Linux CI runner')
    root=Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix='ai-phone-host-acceptance-') as temp:
        home=Path(temp);config=json.loads((root/'config/host-bootstrap.json').read_text());prepare(root)
        payload=root/'apps/client/src-tauri/gen/host/source.payload'
        staged=stage_source(payload,home)
        sdk=home/'tools/android-sdk';licenses=Path(os.environ['ANDROID_HOME'])/'licenses'
        if not licenses.is_dir():raise ValueError('The CI image has no prior SDK license acceptance')
        shutil.copytree(licenses,sdk/'licenses')
        request=home/'request.json';request.write_text(json.dumps({'locale':'en','phone':{'cpus':2,'memory_mb':2048,'storage_gb':16},'desktop':{'cpus':2,'memory_mb':2048}}))
        mise=download(config['mise']['linux-x64'],home/'tools/bin/mise');mise.chmod(0o700)
        env={**os.environ,**{key:str(home/'tools'/value) for key,value in {'MISE_DATA_DIR':'mise/data','MISE_CACHE_DIR':'mise/cache','MISE_CONFIG_DIR':'mise/config','CARGO_HOME':'cargo','RUSTUP_HOME':'rustup'}.items()},'MISE_YES':'1','MISE_PARANOID':'1','CARGO_BUILD_JOBS':'2'}
        command=[str(mise),'exec','-y','python@3.13.5','--','python',str(root/'scripts/client_host_entry.py'),'--payload',str(payload),'--home',str(home),'--request',str(request),'--mise',str(mise)]
        try:
            # Empty stdin fails on an unreviewed new SDK license instead of accepting it.
            subprocess.run(command,stdin=subprocess.DEVNULL,env=env,check=True,timeout=3000)
            request=urllib.request.Request('http://127.0.0.1:17860/api/setup/status',method='POST',data=b'',headers={'Origin':'http://127.0.0.1:17860','Content-Type':'application/x-protobuf'})
            with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request,timeout=8) as response:
                if response.status!=200 or response.headers.get('Content-Type')!='application/x-protobuf':raise ValueError('Fresh setup endpoint not usable')
            # The worker's ready event already requires a real root phone PNG and desktop PNG.
            print('LINUX_HOST_ACCEPTANCE: real root phone, desktop frame, local setup handoff',flush=True)
            sys.path.insert(0,str(staged/'scripts'))
            from linux_services import unit_state
            before={name:unit_state(staged,name)['pid'] for name in ['emulator','runner','media','assistant','channels','web','lan']}
            subprocess.run(command,stdin=subprocess.DEVNULL,env=env,check=True,timeout=300)
            after={name:unit_state(staged,name)['pid'] for name in before}
            if before!=after:raise ValueError('Resume replaced an already running owned service')
            print('LINUX_HOST_RESUME: all seven supervisor PIDs preserved',flush=True)
        finally:
            subprocess.run([sys.executable,str(staged/'scripts/service_stop.py')],cwd=staged,env=env,check=False,timeout=90)


if __name__=='__main__':main()
