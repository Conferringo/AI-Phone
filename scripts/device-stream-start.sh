#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 - "$project_root" <<'PY'
import os,plistlib,subprocess,sys
from pathlib import Path
root=Path(sys.argv[1]);folder=root/'data/device-stream';folder.mkdir(parents=True,exist_ok=True,mode=0o700)
sys.path.insert(0,str(root/'scripts'))
from runtime_config import runtime_env
from prebuilt_js import launch_spec
runtime=runtime_env(root)
spec=launch_spec(root,'media')
config={'Label':'dev.aiphone.device-stream','ProgramArguments':spec['argv'],'WorkingDirectory':str(root),'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'EnvironmentVariables':{**runtime,**spec['environment']},'StandardOutPath':str(folder/'service.log'),'StandardErrorPath':str(folder/'service.log')}
source=folder/'service.plist';source.write_bytes(plistlib.dumps(config));source.chmod(0o600)
dest=Path.home()/'Library/LaunchAgents/dev.aiphone.device-stream.plist';dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(plistlib.dumps(config));dest.chmod(0o600)
domain=f'gui/{os.getuid()}';target=domain+'/'+config['Label']
if subprocess.run(['launchctl','print',target],capture_output=True).returncode==0:subprocess.run(['launchctl','kickstart','-k',target],check=True)
else:subprocess.run(['launchctl','bootstrap',domain,str(dest)],check=True)
PY
