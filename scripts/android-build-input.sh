#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
exec "$AI_PHONE_PROJECT_ROOT/scripts/bun" "$AI_PHONE_PROJECT_ROOT/apps/android-input/build.ts"

