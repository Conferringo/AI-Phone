#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 - "$project_root" "$(command -v node)" <<'PY'
from pathlib import Path
import plistlib,sys
root=Path(sys.argv[1]);folder=root/'data/phone-runner';folder.mkdir(parents=True,exist_ok=True)
sys.path.insert(0,str(root/'scripts'))
from runtime_config import runtime_env
runtime=runtime_env(root)
config={'Label':'dev.aiphone.lan','ProgramArguments':[sys.argv[2],str(root/'scripts/lan-server.mjs')],'WorkingDirectory':str(root),'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'KeepAlive':True,'ThrottleInterval':5,'EnvironmentVariables':{**runtime,'AI_PHONE_PROJECT_ROOT':str(root)},'StandardOutPath':str(folder/'lan.log'),'StandardErrorPath':str(folder/'lan.log')}
(folder/'lan.plist').write_bytes(plistlib.dumps(config))
persistent=Path.home()/'Library/LaunchAgents'/('dev.aiphone.lan'+'.plist')
persistent.parent.mkdir(parents=True,exist_ok=True)
persistent.write_bytes(plistlib.dumps(config));persistent.chmod(0o600)

PY
domain="gui/$(id -u)"
if launchctl print "$domain/dev.aiphone.lan" >/dev/null 2>&1; then launchctl kickstart -k "$domain/dev.aiphone.lan"; else launchctl bootstrap "$domain" "$project_root/data/phone-runner/lan.plist"; fi
