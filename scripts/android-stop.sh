#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
"$ANDROID_HOME/platform-tools/adb" -P 15037 -s emulator-5564 emu kill || true
domain="gui/$(id -u)/dev.aiphone.emulator.p2"
if launchctl print "$domain" >/dev/null 2>&1; then launchctl bootout "$domain"; fi
python3 - "$domain" <<'PY'
import socket, subprocess, sys, time
deadline=time.monotonic()+35
while time.monotonic()<deadline:
    loaded=subprocess.run(['launchctl','print',sys.argv[1]],capture_output=True).returncode==0
    with socket.socket() as s:
        s.settimeout(.2); listening=s.connect_ex(('127.0.0.1',5564))==0
    if not loaded and not listening: print('STOPPED: job removed and console port closed'); break
    time.sleep(.25)
else: raise SystemExit('Emulator shutdown not confirmed; refusing immediate restart')
PY

