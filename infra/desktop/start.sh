#!/bin/sh
set -eu
# /dev is recreated on every container boot; the native Clash app owns the TUN lifecycle.
mkdir -p /dev/net
if [ ! -c /dev/net/tun ]; then mknod /dev/net/tun c 10 200; fi
chmod 600 /dev/net/tun
sh /usr/local/bin/device-network-start || printf '%s\n' 'Device network startup failed; check device configuration.' >&2
export HOME=/home/ai
/usr/local/bin/desktop-bridge home-identity
runuser -u ai -- sh /usr/local/lib/ai-phone/home-initialize.sh prepare
cd /home/ai/Files
rm -f /tmp/.X0-lock
Xvfb :0 -screen 0 1280x800x24 -nolisten tcp -ac &
until DISPLAY=:0 xdpyinfo >/dev/null 2>&1; do sleep .2; done
cat > /usr/local/bin/ai-phone-browser <<'BROWSER'
#!/bin/sh
if [ -n "${HTTPS_PROXY:-}" ]; then
  exec chromium --user-data-dir=/home/ai/.config/ai-phone-browser --remote-debugging-port=9222 --remote-debugging-address=127.0.0.1 --no-sandbox --disable-dev-shm-usage --proxy-server="$HTTPS_PROXY" "$@"
fi
exec chromium --user-data-dir=/home/ai/.config/ai-phone-browser --remote-debugging-port=9222 --remote-debugging-address=127.0.0.1 --no-sandbox --disable-dev-shm-usage "$@"
BROWSER
chmod +x /usr/local/bin/ai-phone-browser
runuser -u ai -- sh /usr/local/lib/ai-phone/home-initialize.sh desktop
mkdir -p /tmp/ai-runtime /tmp/ai-cache
chown "$(id -u ai):$(id -g ai)" /tmp/ai-runtime /tmp/ai-cache
chmod 700 /tmp/ai-runtime
rm -f /tmp/ai-runtime/dbus-address
export XDG_RUNTIME_DIR=/tmp/ai-runtime
export XDG_CACHE_HOME=/tmp/ai-cache
export PULSE_SERVER=unix:/tmp/ai-runtime/pulse/native
(for attempt in $(seq 1 60); do [ -s /tmp/ai-runtime/dbus-address ] && break; sleep .25; done; /usr/local/bin/ai-phone-clash --autostart >/tmp/clash-autostart.log 2>&1) &
runuser -u ai -- bun /usr/local/lib/ai-phone/location.js --watch >/tmp/ai-location.log 2>&1 &
exec runuser -u ai -- dbus-run-session sh -c 'printf "%s" "$DBUS_SESSION_BUS_ADDRESS" > /tmp/ai-runtime/dbus-address; gsettings set org.gnome.desktop.interface toolkit-accessibility true; dunst -config /etc/xdg/dunst/dunstrc >/tmp/ai-notifications.log 2>&1 & /usr/local/bin/desktop-bridge audio-supervise >/tmp/ai-audio.log 2>&1 & openbox & xsetroot -solid "#e7ebef"; tint2 & pcmanfm --desktop & wait'
