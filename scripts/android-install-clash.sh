#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
printf '%s' '{"operation":"phone-install"}' | "$AI_PHONE_PROJECT_ROOT/scripts/bun" "$AI_PHONE_PROJECT_ROOT/apps/device-network/native-apps.ts"
