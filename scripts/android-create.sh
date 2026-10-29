#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
mkdir -p "$ANDROID_USER_HOME" "$ANDROID_AVD_HOME" "$AI_PHONE_PROJECT_ROOT/data/android/logs"
if [ -f "$ANDROID_AVD_HOME/ai_phone_p2.ini" ]; then
  echo "AI Phone AVD already exists; preserving its data."
  exit 0
fi
printf 'no\n' | avdmanager create avd --name ai_phone_p2 \
  --package 'system-images;android-35;default;arm64-v8a' \
  --device pixel_7 --path "$ANDROID_AVD_HOME/phone-local-01.avd"
python3 - "$ANDROID_AVD_HOME/phone-local-01.avd/config.ini" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
values = dict(line.split('=', 1) for line in p.read_text().splitlines() if '=' in line)
import os
values.update({'hw.ramSize':os.environ.get('AI_PHONE_PHONE_MEMORY','3072'),'hw.cpu.ncore':os.environ.get('AI_PHONE_PHONE_CPUS','4'),'hw.lcd.width':'720','hw.lcd.height':'1440','hw.lcd.density':'320','disk.dataPartition.size':os.environ.get('AI_PHONE_PHONE_STORAGE','16')+'G','hw.keyboard':'yes','showDeviceFrame':'no','hw.gpu.enabled':'yes','hw.gpu.mode':'host','fastboot.forceColdBoot':'yes'})
p.write_text(''.join(f'{k}={v}\n' for k,v in values.items()))
PY
echo "Created ai_phone_p2 in the project data directory."

