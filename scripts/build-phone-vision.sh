#!/bin/sh
set -eu
cd "$(dirname -- "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
  echo "Native phone OCR currently requires macOS Vision. Native Android UI targets remain available."
  exit 0
fi
mkdir -p target/phone-vision
if [ ! -x target/phone-vision/recognize ] || [ scripts/phone-vision/main.swift -nt target/phone-vision/recognize ]; then
  swiftc -O scripts/phone-vision/main.swift -o target/phone-vision/recognize
fi
