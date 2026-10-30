#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
python3 - "$project_root" <<'PY'
import os,plistlib,secrets,subprocess,sys
from pathlib import Path
root=Path(sys.argv[1])
sys.path.insert(0,str(root/'scripts'))
from runtime_config import runtime_env
from prebuilt_services import launch_spec
spec=launch_spec(root,'assistant');runtime={**runtime_env(root),**spec['environment']}
environment={**os.environ,**runtime}
subprocess.run(['docker','compose','-f','compose.yaml','-f','compose.dev.yaml','up','-d','postgres'],cwd=root,env=environment,check=True)
if not spec['prebuilt']:
 subprocess.run(['cargo','build','-p','ai-phone-assistant','--locked'],cwd=root,env={**environment,'CARGO_BUILD_JOBS':'2'},check=True)
subprocess.run([sys.executable,str(root/'scripts/database.py'),'prepare'],cwd=root,env=environment,check=True)
subprocess.run([*spec['argv'],'--migrate'],cwd=root,env=environment,check=True)
folder=root/'data/assistant';folder.mkdir(mode=0o700,parents=True,exist_ok=True);folder.chmod(0o700)
(root/'data/attachments').mkdir(mode=0o700,parents=True,exist_ok=True)
(root/'data/attachments').chmod(0o700)
if not (folder/'token').exists():
 fd=os.open(folder/'token',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
 with os.fdopen(fd,'w') as f:f.write(secrets.token_hex(32))
config={'Label':'dev.aiphone.assistant','ProgramArguments':spec['argv'],'WorkingDirectory':str(root),'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'EnvironmentVariables':{**runtime,'AI_PHONE_PROJECT_ROOT':str(root)},'StandardOutPath':str(folder/'assistant.log'),'StandardErrorPath':str(folder/'assistant.log')}
(folder/'assistant.plist').write_bytes(plistlib.dumps(config))
persistent=Path.home()/'Library/LaunchAgents'/('dev.aiphone.assistant'+'.plist')
persistent.parent.mkdir(parents=True,exist_ok=True)
persistent.write_bytes(plistlib.dumps(config));persistent.chmod(0o600)

PY
domain="gui/$(id -u)"
if launchctl print "$domain/dev.aiphone.assistant" >/dev/null 2>&1; then launchctl bootout "$domain/dev.aiphone.assistant"; fi
launchctl bootstrap "$domain" "$project_root/data/assistant/assistant.plist"
echo "Started private assistant service on 127.0.0.1:17863."
