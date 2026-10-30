#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
cd "$AI_PHONE_PROJECT_ROOT"
if [ ! -f "$AI_PHONE_PROJECT_ROOT/runtime/manifest.json" ]; then
  NEXT_TELEMETRY_DISABLED=1 ./scripts/bun run build
fi
python3 - "$AI_PHONE_PROJECT_ROOT" "$(command -v node)" <<'PY'
import plistlib, sys
from pathlib import Path
root=Path(sys.argv[1]);folder=root/'data/phone-runner';folder.mkdir(parents=True,exist_ok=True)
sys.path.insert(0,str(root/'scripts'))
from runtime_config import runtime_env
runtime=runtime_env(root)
from prebuilt_web import web_command
prebuilt=(root/'runtime/manifest.json').is_file()
argv=web_command(root) if prebuilt else [sys.argv[2],str(root/'apps/web/node_modules/next/dist/bin/next'),'start','--hostname','127.0.0.1','--port','17860']
config={'Label':'dev.aiphone.web.p2','ProgramArguments':argv,'WorkingDirectory':str(root/'apps/web'),'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'EnvironmentVariables':{**runtime,'AI_PHONE_PROJECT_ROOT':str(root),'NEXT_TELEMETRY_DISABLED':'1','HOSTNAME':'127.0.0.1','PORT':'17860'},'StandardOutPath':str(folder/'web.log'),'StandardErrorPath':str(folder/'web.log')}
(folder/'web.plist').write_bytes(plistlib.dumps(config))
persistent=Path.home()/'Library/LaunchAgents'/('dev.aiphone.web.p2'+'.plist')
persistent.parent.mkdir(parents=True,exist_ok=True)
persistent.write_bytes(plistlib.dumps(config));persistent.chmod(0o600)

PY
domain="gui/$(id -u)"
if launchctl print "$domain/dev.aiphone.web.p2" >/dev/null 2>&1; then
  launchctl kickstart -k "$domain/dev.aiphone.web.p2"
else
  launchctl bootstrap "$domain" "$AI_PHONE_PROJECT_ROOT/data/phone-runner/web.plist"
fi
echo "AI Phone local viewer: http://127.0.0.1:17860/phone"

