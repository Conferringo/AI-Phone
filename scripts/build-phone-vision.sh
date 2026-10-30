#!/bin/sh
set -eu
cd "$(dirname -- "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
  command -v tesseract >/dev/null
  /usr/bin/python3 -c 'from PIL import Image'
  mkdir -p target/phone-vision
  cp scripts/phone-vision/tesseract.py target/phone-vision/recognize
  chmod 755 target/phone-vision/recognize
  exit 0
fi
mkdir -p target/phone-vision
if [ ! -x target/phone-vision/recognize ] || [ scripts/phone-vision/main.swift -nt target/phone-vision/recognize ] || [ scripts/build-phone-vision.sh -nt target/phone-vision/recognize ] || [ scripts/macos_target.py -nt target/phone-vision/recognize ] || [ apps/client/src-tauri/tauri.conf.json -nt target/phone-vision/recognize ]; then
  macos_target=$(python3 scripts/macos_target.py)
  swiftc -O -target "$macos_target" scripts/phone-vision/main.swift -o target/phone-vision/recognize
fi
