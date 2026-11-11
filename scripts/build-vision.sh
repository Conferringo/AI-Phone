#!/bin/sh
set -eu
cd "$(dirname -- "$0")/.."
exec cargo build --locked -p ai-phone-vision --features native
