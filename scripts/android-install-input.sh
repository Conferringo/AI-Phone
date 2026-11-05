#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
if [ ! -e "$AI_PHONE_PROJECT_ROOT/runtime/services" ] && [ ! -L "$AI_PHONE_PROJECT_ROOT/runtime/services" ]; then
  # Maintainer source installs prepare their helper. Complete clients use the
  # qualified bundled assistant and never require Cargo or a compiler.
  (cd "$AI_PHONE_PROJECT_ROOT" && CARGO_BUILD_JOBS=2 cargo build -p ai-phone-host-runtime --bin host-resources --locked)
fi
python3 "$AI_PHONE_PROJECT_ROOT/scripts/android_input.py" \
  --adb "$ANDROID_HOME/platform-tools/adb" \
  --serial "${AI_PHONE_SERIAL:-emulator-5564}" --root "$AI_PHONE_PROJECT_ROOT"
