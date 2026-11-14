#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
if [ "${1:-}" = --observe-only ]; then
  "$AI_PHONE_PROJECT_ROOT/scripts/android-maintenance" ready --observe-only
else
  "$AI_PHONE_PROJECT_ROOT/scripts/android-maintenance" ready
  "$AI_PHONE_PROJECT_ROOT/scripts/android-configure-network.sh"
fi
