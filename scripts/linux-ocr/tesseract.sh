#!/bin/sh
set -eu
parent=${0%/*}
root=$(CDPATH= cd -- "$parent/.." && pwd)
exec "$root/lib/ld-linux-x86-64.so.2" --library-path "$root/lib" "$root/bin/tesseract" "$@"
