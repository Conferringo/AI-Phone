#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
cd "$AI_PHONE_PROJECT_ROOT"
archive=data/android/downloads/curl-linux-aarch64-musl-8.23.0-1.tar.xz
printf '%s  %s\n' e20f26e5b87c1883608f286f86bdabf39d3b8556e8a8e75dfe205e14bd206199 "$archive" | shasum -a 256 -c -
mkdir -p data/android/downloads/curl-static
tar -xJf "$archive" -C data/android/downloads/curl-static
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" remount
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell mkdir -p /data/local/ai-phone/bin
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" push data/android/downloads/curl-static/curl /data/local/ai-phone/bin/curl
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" push scripts/android-curl /system/bin/curl
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell 'chmod 755 /data/local/ai-phone/bin/curl /system/bin/curl; chown root:root /system/bin/curl; restorecon /system/bin/curl'
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell 'printf "nameserver 10.0.2.3\n" > /system/etc/resolv.conf; chmod 644 /system/etc/resolv.conf; restorecon /system/etc/resolv.conf'
adb -P 15037 -s "${AI_PHONE_SERIAL:-emulator-5564}" shell curl --version
