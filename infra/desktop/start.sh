#!/bin/sh
set -eu
# /dev is recreated on every container boot; the native Clash app owns the TUN lifecycle.
mkdir -p /dev/net
if [ ! -c /dev/net/tun ]; then mknod /dev/net/tun c 10 200; fi
chmod 600 /dev/net/tun
sh /usr/local/bin/device-network-start || printf '%s\n' 'Device network startup failed; check device configuration.' >&2
mkdir -p /home/ai/Files /home/ai/Desktop /home/ai/.config/openbox /home/ai/.config/pcmanfm/default
if [ ! -f /home/ai/.config/pcmanfm/.ai-phone-style-v1 ]; then
cat > /home/ai/.config/pcmanfm/default/desktop-items-0.conf <<'DESKTOP_STYLE'
[*]
wallpaper_mode=color
desktop_bg=#e7ebef
desktop_fg=#26313c
desktop_shadow=#ffffff
show_documents=0
show_trash=0
show_mounts=0
DESKTOP_STYLE
touch /home/ai/.config/pcmanfm/.ai-phone-style-v1
fi
find /home/ai \( -type d -o -type f \) -exec chown ai:ai {} +
rm -f /tmp/.X0-lock
# Only this container owns the persisted GUI profile; no office process exists at boot.
if [ ! -f /home/ai/.config/.ai-phone-office-v1 ]; then
  if [ -d /home/ai/.config/libreoffice ]; then mv /home/ai/.config/libreoffice /home/ai/.config/libreoffice-before-desktop-fix; fi
  touch /home/ai/.config/.ai-phone-office-v1
  chown ai:ai /home/ai/.config/.ai-phone-office-v1
fi
rm -f /home/ai/.config/libreoffice/4/.lock
mkdir -p /home/ai/Templates
Xvfb :0 -screen 0 1280x800x24 -nolisten tcp -ac &
until DISPLAY=:0 xdpyinfo >/dev/null 2>&1; do sleep .2; done
cat > /home/ai/Desktop/Files.desktop <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=工作文件
Exec=pcmanfm /home/ai/Files
Icon=folder
Terminal=false
DESKTOP
cat > /home/ai/Desktop/Calc.desktop <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=表格
Exec=libreoffice --calc
Icon=libreoffice-calc
Terminal=false
DESKTOP
cat > /home/ai/Desktop/Browser.desktop <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=浏览器
Exec=/usr/local/bin/ai-phone-browser
Icon=chromium
Terminal=false
DESKTOP
cat > /usr/local/bin/ai-phone-browser <<'BROWSER'
#!/bin/sh
if [ -n "${HTTPS_PROXY:-}" ]; then
  exec chromium --no-sandbox --disable-dev-shm-usage --proxy-server="$HTTPS_PROXY" "$@"
fi
exec chromium --no-sandbox --disable-dev-shm-usage "$@"
BROWSER
cat > /home/ai/Desktop/Clash.desktop <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Clash
Exec=/usr/local/bin/ai-phone-clash
Icon=clash-party
Terminal=false
DESKTOP
chmod +x /usr/local/bin/ai-phone-browser
chmod +x /home/ai/Desktop/*.desktop
chown -R ai:ai /home/ai/Desktop
mkdir -p /tmp/ai-runtime /tmp/ai-cache
chown ai:ai /tmp/ai-runtime /tmp/ai-cache
chmod 700 /tmp/ai-runtime
rm -f /tmp/ai-runtime/dbus-address
export XDG_RUNTIME_DIR=/tmp/ai-runtime
export XDG_CACHE_HOME=/tmp/ai-cache
export PULSE_SERVER=unix:/tmp/ai-runtime/pulse/native
(for attempt in $(seq 1 60); do [ -s /tmp/ai-runtime/dbus-address ] && break; sleep .25; done; /usr/local/bin/ai-phone-clash --autostart >/tmp/clash-autostart.log 2>&1) &
exec runuser -u ai -- dbus-run-session sh -c 'printf "%s" "$DBUS_SESSION_BUS_ADDRESS" > /tmp/ai-runtime/dbus-address; python3 /usr/local/lib/ai-phone/audio-service.py >/tmp/ai-audio.log 2>&1 & openbox & xsetroot -solid "#e7ebef"; tint2 & pcmanfm --desktop & wait'
