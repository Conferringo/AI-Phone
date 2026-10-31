#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
if [ "${1:-}" = --observe-only ]; then
  python3 "$AI_PHONE_PROJECT_ROOT/scripts/android_boot.py" "$ANDROID_HOME/platform-tools/adb" --observe-only
else
  python3 "$AI_PHONE_PROJECT_ROOT/scripts/android_boot.py" "$ANDROID_HOME/platform-tools/adb"
  "$AI_PHONE_PROJECT_ROOT/scripts/android-configure-network.sh"
fi
