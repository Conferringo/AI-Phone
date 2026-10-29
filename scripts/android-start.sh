#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
mkdir -p "$AI_PHONE_PROJECT_ROOT/data/android/logs" "$AI_PHONE_PROJECT_ROOT/data/android/run"
adb -P 15037 start-server
python3 - "$AI_PHONE_PROJECT_ROOT" "$AI_PHONE_EMULATOR_PROXY" <<'PY'
import plistlib, sys
from pathlib import Path
root = Path(sys.argv[1])
p = root / 'data/android/run/emulator.plist'
config = {'Label':'dev.aiphone.emulator.p2', 'ProgramArguments':['/bin/sh', str(root/'scripts/android-emulator.sh')], 'WorkingDirectory':str(root), 'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5, 'EnvironmentVariables':{'AI_PHONE_EMULATOR_PROXY':sys.argv[2]}, 'StandardOutPath':str(root/'data/android/logs/emulator.log'), 'StandardErrorPath':str(root/'data/android/logs/emulator.log')}
p.write_bytes(plistlib.dumps(config))
persistent=Path.home()/'Library/LaunchAgents'/('dev.aiphone.emulator.p2'+'.plist')
persistent.parent.mkdir(parents=True,exist_ok=True)
persistent.write_bytes(plistlib.dumps(config));persistent.chmod(0o600)

PY
domain="gui/$(id -u)"
if launchctl print "$domain/dev.aiphone.emulator.p2" >/dev/null 2>&1; then
  launchctl kickstart "$domain/dev.aiphone.emulator.p2"
else
  launchctl bootstrap "$domain" "$AI_PHONE_PROJECT_ROOT/data/android/run/emulator.plist"
fi
echo "Started login-session launchd job dev.aiphone.emulator.p2 (not installed for future logins)."
sleep 1
launchctl print "$domain/dev.aiphone.emulator.p2" >/dev/null

