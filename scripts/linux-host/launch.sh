#!/bin/sh
set -eu
parent=${0%/*}
root=$(CDPATH= cd -- "$parent/.." && pwd)
name=${0##*/}
case "$name" in
  qemu-system-x86_64) set -- -L "$root/share/qemu" "$@" ;;
  qemu-img|ssh|ssh-keygen) ;;
  scp) set -- -S "$root/launchers/ssh" "$@" ;;
  *) exit 64 ;;
esac
export QEMU_MODULE_DIR="$root/modules"
exec "$root/lib/ld-linux-x86-64.so.2" --library-path "$root/lib" "$root/bin/$name" "$@"
