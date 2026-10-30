#!/bin/sh
set -eu
launchctl_command=$(command -v launchctl)
. "$(dirname -- "$0")/android-env.sh"
cd "$AI_PHONE_PROJECT_ROOT"
python3 - "$AI_PHONE_PROJECT_ROOT" <<'PY'
import os, plistlib, secrets, subprocess, sys
from pathlib import Path
root = Path(sys.argv[1])
sys.path.insert(0,str(root/'scripts'))
from runtime_config import runtime_env
from prebuilt_services import launch_spec
spec=launch_spec(root,'runner');runtime={**runtime_env(root),**spec['environment']}
environment={**os.environ,**runtime}
if not spec['prebuilt']:
    subprocess.run(['sh',str(root/'scripts/build-phone-vision.sh')],cwd=root,env=environment,check=True)
    subprocess.run(['cargo','build','-p','ai-phone-runner','--locked'],cwd=root,env=environment,check=True)
folder = root/'data/phone-runner'; folder.mkdir(mode=0o700, parents=True, exist_ok=True); folder.chmod(0o700)
token = folder/'token'
if not token.exists():
    fd = os.open(token, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd,'w') as f: f.write(secrets.token_hex(32))
config = {'Label':'dev.aiphone.runner.p2', 'ProgramArguments':spec['argv'], 'WorkingDirectory':str(root), 'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5, 'EnvironmentVariables':{**runtime,'AI_PHONE_PROJECT_ROOT':str(root),'ANDROID_USER_HOME':str(root/'data/android/user')}, 'StandardOutPath':str(folder/'runner.log'), 'StandardErrorPath':str(folder/'runner.log')}
(folder/'runner.plist').write_bytes(plistlib.dumps(config))
persistent=Path.home()/'Library/LaunchAgents'/('dev.aiphone.runner.p2'+'.plist')
persistent.parent.mkdir(parents=True,exist_ok=True)
persistent.write_bytes(plistlib.dumps(config));persistent.chmod(0o600)

PY
domain="gui/$(id -u)"
if "$launchctl_command" print "$domain/dev.aiphone.runner.p2" >/dev/null 2>&1; then
  "$launchctl_command" bootout "$domain/dev.aiphone.runner.p2"
fi
"$launchctl_command" bootstrap "$domain" "$AI_PHONE_PROJECT_ROOT/data/phone-runner/runner.plist"
echo "Started loopback-only phone runner; pairing token remains in data/phone-runner/token."
