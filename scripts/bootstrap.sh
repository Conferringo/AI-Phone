#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
if [ ! -x .tools/node_modules/.bin/bun ]; then
  npm install --prefix .tools --no-audit --no-fund --package-lock=false bun@1.4.4
fi
actual_version=$(.tools/node_modules/.bin/bun --version)
if [ "$actual_version" != "1.4.4" ]; then
  echo "Expected Bun 1.4.4, found $actual_version" >&2
  exit 1
fi
if [ -f bun.lock ]; then
  ./scripts/bun install --frozen-lockfile
else
  ./scripts/bun install
fi
./scripts/bun run proto:generate

