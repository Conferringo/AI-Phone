#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
if [ ! -e "$AI_PHONE_PROJECT_ROOT/runtime/services" ] && [ ! -L "$AI_PHONE_PROJECT_ROOT/runtime/services" ]; then
  "$AI_PHONE_PROJECT_ROOT/scripts/bun" "$AI_PHONE_PROJECT_ROOT/scripts/artifacts.ts" phone --build
  apk="$AI_PHONE_PROJECT_ROOT/.tools/phone-apps/input.apk"
  probe="$AI_PHONE_PROJECT_ROOT/.tools/phone-apps/notification-access.jar"
else
  apk="$AI_PHONE_PROJECT_ROOT/runtime/phone-apps/input.apk"
  probe="$AI_PHONE_PROJECT_ROOT/runtime/phone-apps/notification-access.jar"
fi
exec "$AI_PHONE_PROJECT_ROOT/scripts/android-maintenance" input "$apk" "$probe"
