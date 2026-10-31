#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
python3 "$AI_PHONE_PROJECT_ROOT/scripts/android_input.py" \
  --adb "$ANDROID_HOME/platform-tools/adb" \
  --serial "${AI_PHONE_SERIAL:-emulator-5564}" --root "$AI_PHONE_PROJECT_ROOT"
