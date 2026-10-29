#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 - "$project_root" <<'PY'
import os,plistlib,subprocess,sys
from pathlib import Path
root=Path(sys.argv[1]);folder=root/'data/channels';folder.mkdir(parents=True,exist_ok=True,mode=0o700)
sys.path.insert(0,str(root/'scripts'))
from runtime_config import runtime_env
runtime=runtime_env(root)
config={'Label':'dev.aiphone.channels','ProgramArguments':[str(root/'.tools/node_modules/.bin/bun'),str(root/'apps/channels/server.ts')],'WorkingDirectory':str(root),'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'EnvironmentVariables':{**runtime,'AI_PHONE_PROJECT_ROOT':str(root)},'StandardOutPath':str(folder/'service.log'),'StandardErrorPath':str(folder/'service.log')}
source=folder/'service.plist';source.write_bytes(plistlib.dumps(config));source.chmod(0o600)
destination=Path.home()/'Library/LaunchAgents/dev.aiphone.channels.plist';destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(plistlib.dumps(config));destination.chmod(0o600)
domain=f'gui/{os.getuid()}';target=domain+'/'+config['Label']
if subprocess.run(['launchctl','print',target],capture_output=True).returncode==0:subprocess.run(['launchctl','kickstart','-k',target],check=True)
else:subprocess.run(['launchctl','bootstrap',domain,str(destination)],check=True)
print('Started private channel adapters on 127.0.0.1:17864.')
PY
