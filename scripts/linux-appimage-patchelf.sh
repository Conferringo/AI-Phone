#!/bin/sh
# linuxdeploy may relocate GUI libraries; sealed host runtimes keep their own ABI.
set -eu
for target do :; done
case "$1:$target" in
  --set-rpath:*/usr/lib/AI\ Phone/host/*)
    printf '%s\n' 'Preserving sealed runtime RPATH.' >&2
    exit 0
    ;;
esac
exec "${AI_PHONE_REAL_PATCHELF:?Missing native patchelf}" "$@"
