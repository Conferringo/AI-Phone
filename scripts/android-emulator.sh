#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
# Read applied AVD resources: changing the host profile must not silently resize an existing phone.
avd_config="$ANDROID_AVD_HOME/phone-local-01.avd/config.ini"
applied_resource() {
  awk -F '[[:space:]]*=[[:space:]]*' -v key="$1" '$1==key {sub(/[[:space:]]+$/, "", $2); print $2; exit}' "$avd_config"
}
phone_memory=$(applied_resource hw.ramSize)
phone_cpus=$(applied_resource hw.cpu.ncore)
if [ -n "$AI_PHONE_EMULATOR_PROXY" ]; then set -- -http-proxy "$AI_PHONE_EMULATOR_PROXY"; else set --; fi
if [ -n "${AI_PHONE_EMULATOR_DNS:-}" ]; then set -- "$@" -dns-server "$AI_PHONE_EMULATOR_DNS"; fi
phone_gpu=host
if [ "$(uname -s)" = Linux ]; then phone_gpu=swiftshader_indirect; fi
exec "$ANDROID_HOME/emulator/emulator" -avd ai_phone_p2 -port 5564 \
  -no-window -audio none -grpc 8558 -grpc-use-token -no-boot-anim -no-snapshot -writable-system \
  -feature -DeviceSkinOverlay -gpu "$phone_gpu" -memory "$phone_memory" -cores "$phone_cpus" \
  -netspeed full -netdelay none \
  "$@"

