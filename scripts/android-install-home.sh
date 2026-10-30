#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
python3 "$AI_PHONE_PROJECT_ROOT/apps/android-home/build.py"
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" install -r "$AI_PHONE_PROJECT_ROOT/data/android/home/ai-phone-home.apk"
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell cmd package set-home-activity dev.aiphone.home/.HomeActivity
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell cmd input keyevent KEYCODE_HOME
