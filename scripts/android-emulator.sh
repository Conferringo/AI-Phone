#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
# Read applied AVD resources: changing the host profile must not silently resize an existing phone.
eval "$(python3 - "$ANDROID_AVD_HOME/phone-local-01.avd/config.ini" <<'PY'
import shlex,sys
from pathlib import Path
values=dict(line.split('=',1) for line in Path(sys.argv[1]).read_text().splitlines() if '=' in line)
for name,key,default in [('phone_memory','hw.ramSize','3072'),('phone_cpus','hw.cpu.ncore','4')]:
 print(name+'='+shlex.quote(values.get(key,default)))
PY
)"
if [ -n "$AI_PHONE_EMULATOR_PROXY" ]; then set -- -http-proxy "$AI_PHONE_EMULATOR_PROXY"; else set --; fi
if [ -n "${AI_PHONE_EMULATOR_DNS:-}" ]; then set -- "$@" -dns-server "$AI_PHONE_EMULATOR_DNS"; fi
exec "$ANDROID_HOME/emulator/emulator" -avd ai_phone_p2 -port 5564 \
  -no-window -audio none -grpc 8558 -grpc-use-token -no-boot-anim -no-snapshot -writable-system \
  -feature -DeviceSkinOverlay -gpu host -memory "$phone_memory" -cores "$phone_cpus" \
  -netspeed full -netdelay none \
  "$@"

