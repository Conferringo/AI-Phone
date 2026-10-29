#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
# Keep one TCP proxy at the emulator layer. Applying the Binder setter also
# notifies running apps; writing settings alone does not verify runtime state.
helper="$AI_PHONE_PROJECT_ROOT/data/android/network-helper/network-proxy.jar"
if [ ! -f "$helper" ] || [ "$AI_PHONE_PROJECT_ROOT/scripts/android-network/NetworkProxy.java" -nt "$helper" ] || [ "$AI_PHONE_PROJECT_ROOT/scripts/android-network/DataUsageWarnings.java" -nt "$helper" ]; then
  "$AI_PHONE_PROJECT_ROOT/scripts/android-build-network.sh"
fi
"$AI_PHONE_PROJECT_ROOT/scripts/android-adb" push "$helper" /data/local/tmp/ai-phone-network-proxy.jar >/dev/null
"$AI_PHONE_PROJECT_ROOT/scripts/android-adb" shell 'settings put global http_proxy :0; CLASSPATH=/data/local/tmp/ai-phone-network-proxy.jar app_process / dev.aiphone.network.NetworkProxy --clear'
"$AI_PHONE_PROJECT_ROOT/scripts/android-adb" shell 'for interface in all default eth0; do echo 1 > /proc/sys/net/ipv6/conf/$interface/disable_ipv6; done; test "$(cat /proc/sys/net/ipv6/conf/eth0/disable_ipv6)" = 1'
"$AI_PHONE_PROJECT_ROOT/scripts/android-adb" shell 'CLASSPATH=/data/local/tmp/ai-phone-network-proxy.jar app_process / dev.aiphone.network.DataUsageWarnings --disable-warnings'
printf '%s\n' 'Android network ready: configured emulator route; IPv4; no duplicate system proxy.'
