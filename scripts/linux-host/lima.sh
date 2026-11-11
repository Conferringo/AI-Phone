#!/bin/sh
set -eu
parent=${0%/*}
root=$(CDPATH= cd -- "$parent/.." && pwd)
export PATH="$root/native/launchers:${PATH:-/usr/bin:/bin}"
exec "$root/lima/bin/limactl" "$@"
