#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
# Read applied AVD resources: changing the host profile must not silently resize an existing phone.
phone_memory=$(awk -F= '$1=="hw.ramSize" {print $2; exit}' "$ANDROID_AVD_HOME/phone-local-01.avd/config.ini")
phone_cpus=$(awk -F= '$1=="hw.cpu.ncore" {print $2; exit}' "$ANDROID_AVD_HOME/phone-local-01.avd/config.ini")
if [ -n "$AI_PHONE_EMULATOR_PROXY" ]; then set -- -http-proxy "$AI_PHONE_EMULATOR_PROXY"; else set --; fi
if [ -n "${AI_PHONE_EMULATOR_DNS:-}" ]; then set -- "$@" -dns-server "$AI_PHONE_EMULATOR_DNS"; fi
phone_gpu=host
if [ "$(uname -s)" = Linux ]; then phone_gpu=swiftshader_indirect; fi
exec "$ANDROID_HOME/emulator/emulator" -avd ai_phone_p2 -port 5564 \
  -no-window -audio none -grpc 8558 -grpc-use-token -no-boot-anim -no-snapshot -writable-system \
  -feature -DeviceSkinOverlay -gpu "$phone_gpu" -memory "$phone_memory" -cores "$phone_cpus" \
  -netspeed full -netdelay none \
  "$@"

