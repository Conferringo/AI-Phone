#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
apk=$(python3 "$AI_PHONE_PROJECT_ROOT/scripts/prebuilt_phone.py" --root "$AI_PHONE_PROJECT_ROOT" --app home)
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" install -r "$apk"
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell cmd package set-home-activity dev.aiphone.home/.HomeActivity
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell cmd input keyevent KEYCODE_HOME
