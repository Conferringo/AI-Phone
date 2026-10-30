#!/bin/sh
set -eu
cd "$(dirname -- "$0")"
if [ ! -f data/deployment/profile.json ]; then
  ./ai-phone install
else
  ./ai-phone start
fi
