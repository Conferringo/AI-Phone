#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
exec python3 "$AI_PHONE_PROJECT_ROOT/apps/android-input/build.py"

