#!/bin/sh
set -eu
folder=/etc/ai-phone-network
core=/opt/ai-phone/network-core/mihomo
if [ ! -f "$folder/enabled" ] || [ ! -s "$folder/config.json" ] || [ ! -x "$core" ]; then exit 0; fi
mkdir -p /dev/net
[ -c /dev/net/tun ] || mknod /dev/net/tun c 10 200
chmod 600 /dev/net/tun
ip rule show | grep -q 'fwmark 0x2510 lookup main' || ip rule add priority 8990 fwmark 9488 table main
nohup sh "$folder/supervisor.sh" "$folder" "$core" >/dev/null 2>&1 </dev/null &
