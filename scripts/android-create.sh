#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
mkdir -p "$ANDROID_USER_HOME" "$ANDROID_AVD_HOME" "$AI_PHONE_DATA_ROOT/android/logs"
python3 "$AI_PHONE_PROJECT_ROOT/scripts/android_avd.py"
