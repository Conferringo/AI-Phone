#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
python3 scripts/bootstrap_bun.py
if [ -f bun.lock ]; then
  ./scripts/bun install --frozen-lockfile
else
  ./scripts/bun install
fi
./scripts/bun run proto:generate

./scripts/bun run location:generate
