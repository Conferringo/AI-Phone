#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
apk=$(python3 "$AI_PHONE_PROJECT_ROOT/scripts/prebuilt_phone.py" --root "$AI_PHONE_PROJECT_ROOT" --app input)
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" install -r "$apk"
# Updating an IME package can reset the selected keyboard, so select after install.
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell ime enable dev.aiphone.input/.UnicodeIme
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell ime set dev.aiphone.input/.UnicodeIme
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell settings put secure show_ime_with_hard_keyboard 1

adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell cmd notification allow_listener dev.aiphone.input/.PhoneNotifications
python3 - "$ANDROID_HOME/platform-tools/adb" <<'PY'
import os, subprocess, sys
adb=[sys.argv[1],'-P','15037','-s',os.environ.get('AI_PHONE_SERIAL','emulator-5564')]
current=subprocess.check_output(adb+['shell','settings','get','secure','enabled_accessibility_services'],text=True).strip()
services=[value for value in current.split(':') if value and value!='null']
service='dev.aiphone.input/.PhoneAttention'
if service not in services: services.append(service)
subprocess.run(adb+['shell','settings','put','secure','enabled_accessibility_services',':'.join(services)],check=True)
subprocess.run(adb+['shell','settings','put','secure','accessibility_enabled','1'],check=True)
PY
