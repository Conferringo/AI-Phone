#!/bin/sh
set -eu
export HOME=/home/ai
export XDG_DATA_HOME=/home/ai/.local/share
export DISPLAY=${DISPLAY:-:0}
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
if [ "$(id -u)" != 0 ]; then exec sudo -n -E /usr/local/bin/ai-phone-clash "$@"; fi
if [ "${1:-}" = --autostart ]; then
  shift
  found=0
  for entry in /home/ai/.config/autostart/*.desktop; do
    [ -f "$entry" ] || continue
    if grep -Eq '^Exec=.*clash-verge' "$entry" && ! grep -Eq '^(Hidden=true|X-GNOME-Autostart-enabled=false)' "$entry"; then found=1;break;fi
  done
  [ "$found" = 1 ] || exit 0
fi
if [ -s /tmp/ai-runtime/dbus-address ]; then export DBUS_SESSION_BUS_ADDRESS=$(cat /tmp/ai-runtime/dbus-address); fi
exec /usr/bin/clash-verge "$@"
