#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
cd "$AI_PHONE_PROJECT_ROOT"
./scripts/build-phone-vision.sh
cargo build -p ai-phone-runner --locked
python3 - "$AI_PHONE_PROJECT_ROOT" <<'PY'
import os, plistlib, secrets, sys
from pathlib import Path
root = Path(sys.argv[1]); folder = root/'data/phone-runner'; folder.mkdir(mode=0o700, parents=True, exist_ok=True); folder.chmod(0o700)
token = folder/'token'
if not token.exists():
    fd = os.open(token, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd,'w') as f: f.write(secrets.token_hex(32))
sys.path.insert(0,str(root/'scripts'))
from runtime_config import runtime_env
runtime=runtime_env(root)
config = {'Label':'dev.aiphone.runner.p2', 'ProgramArguments':[str(root/'target/debug/ai-phone-runner')], 'WorkingDirectory':str(root), 'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5, 'EnvironmentVariables':{**runtime,'AI_PHONE_PROJECT_ROOT':str(root),'ANDROID_USER_HOME':str(root/'data/android/user')}, 'StandardOutPath':str(folder/'runner.log'), 'StandardErrorPath':str(folder/'runner.log')}
(folder/'runner.plist').write_bytes(plistlib.dumps(config))
persistent=Path.home()/'Library/LaunchAgents'/('dev.aiphone.runner.p2'+'.plist')
persistent.parent.mkdir(parents=True,exist_ok=True)
persistent.write_bytes(plistlib.dumps(config));persistent.chmod(0o600)

PY
domain="gui/$(id -u)"
if launchctl print "$domain/dev.aiphone.runner.p2" >/dev/null 2>&1; then
  launchctl bootout "$domain/dev.aiphone.runner.p2"
fi
launchctl bootstrap "$domain" "$AI_PHONE_PROJECT_ROOT/data/phone-runner/runner.plist"
echo "Started loopback-only phone runner; pairing token remains in data/phone-runner/token."
