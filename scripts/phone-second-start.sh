#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
[ -f "$project_root/data/phones/second.json" ] || exit 0
[ ! -f "$project_root/data/phones/second/archived" ] || exit 0
domain="gui/$(id -u)"
for service in dev.aiphone.emulator.second dev.aiphone.runner.second dev.aiphone.media.second; do
  config="$project_root/data/phones/second/$service.plist"
  [ -f "$config" ] || continue
  if ! launchctl print "$domain/$service" >/dev/null 2>&1; then launchctl bootstrap "$domain" "$config"; fi
  if [ "$service" = dev.aiphone.emulator.second ]; then
    AI_PHONE_SERIAL=emulator-5566 "$project_root/scripts/android-ready.sh"
  fi
done
