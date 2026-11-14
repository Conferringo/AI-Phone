#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
mkdir -p "$ANDROID_USER_HOME" "$ANDROID_AVD_HOME" "$AI_PHONE_DATA_ROOT/android/logs"
exec "$AI_PHONE_PROJECT_ROOT/scripts/android-maintenance" create "$AI_PHONE_SDK_ROOT" "$ANDROID_AVD_HOME" "$AI_PHONE_ANDROID_ABI" "$AI_PHONE_PHONE_CPUS" "$AI_PHONE_PHONE_MEMORY" "$AI_PHONE_PHONE_STORAGE"
