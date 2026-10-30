#!/bin/sh
set -eu
export HOME=/home/ai
export DISPLAY=${DISPLAY:-:0}
export ELECTRON_DISABLE_SANDBOX=1
if [ "$(id -u)" != 0 ]; then exec sudo -n -E /usr/local/bin/ai-phone-clash "$@"; fi
if [ "${1:-}" = --autostart ]; then
  shift
  found=0
  for entry in /home/ai/.config/autostart/*.desktop; do
    [ -f "$entry" ] || continue
    if grep -Eq '^Exec=.*(clash-party|mihomo-party)' "$entry" && ! grep -Eq '^(Hidden=true|X-GNOME-Autostart-enabled=false)' "$entry"; then found=1;break;fi
  done
  [ "$found" = 1 ] || exit 0
fi
if [ -s /tmp/ai-runtime/dbus-address ]; then export DBUS_SESSION_BUS_ADDRESS=$(cat /tmp/ai-runtime/dbus-address); fi
# Only clear this application's stale single-instance files when it is not running.
if ! pgrep -f '^(/usr/bin/clash-party|/opt/clash-party/mihomo-party)( --no-sandbox)?$' >/dev/null; then
  rm -f /home/ai/.config/mihomo-party/SingletonLock /home/ai/.config/mihomo-party/SingletonCookie /home/ai/.config/mihomo-party/SingletonSocket
fi
exec /usr/bin/clash-party --no-sandbox "$@"
