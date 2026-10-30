#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
python3 - "$ANDROID_HOME/platform-tools/adb" "${1:-}" <<'PY'
import os, subprocess, sys, time
adb = [sys.argv[1], '-P', '15037', '-s', os.environ.get('AI_PHONE_SERIAL','emulator-5564')]
observe_only=sys.argv[2]=='--observe-only'
def run(*args):
    try: return subprocess.run(adb+list(args), capture_output=True, text=True, timeout=6)
    except subprocess.TimeoutExpired: return None
deadline = time.monotonic()+120
while time.monotonic()<deadline:
    result = run('shell','getprop','sys.boot_completed')
    if result and result.returncode==0 and result.stdout.strip()=='1': break
    time.sleep(1)
else: raise SystemExit('Android did not finish booting within 120 seconds')
if not observe_only:run('root')
for _ in range(20):
    result = run('shell','id','-u')
    if result and result.returncode==0 and result.stdout.strip()=='0':
        if observe_only:
            print('READY: existing Android/root observed; settings unchanged')
            break
        run('shell','settings','put','system','show_touches','1')
        touches=run('shell','settings','get','system','show_touches')
        if not touches or touches.returncode!=0 or touches.stdout.strip()!='1':
            raise SystemExit('Native Android touch indicators could not be enabled')
        overlays=run('shell','cmd','overlay','list')
        if overlays:
            for line in overlays.stdout.splitlines():
                if line.startswith('[x] com.android.internal.display.cutout.emulation.') or line in ['[x] com.android.internal.emulation.pixel_7','[x] com.android.systemui.emulation.pixel_7']:
                    run('shell','cmd','overlay','disable','--user','0',line.split()[-1])
        print('READY: real Android boot completed; control channel uid=0; no emulated display cutout')
        break
    time.sleep(0.5)
else: raise SystemExit('Android is not root-ready')
PY

if [ "${1:-}" != --observe-only ]; then "$AI_PHONE_PROJECT_ROOT/scripts/android-configure-network.sh"; fi
