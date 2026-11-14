#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' config/bun-runtime.json)
bun_bin="$project_root/.tools/bin/bun"
case "$(uname -s)" in Darwin) host=darwin;; Linux) host=linux;; MINGW*|MSYS*|CYGWIN*) host=windows; bun_bin="$bun_bin.exe";; *) exit 1;; esac
case "$(uname -m)" in arm64|aarch64) arch=aarch64; baseline=;; x86_64|AMD64) arch=x64; baseline=-baseline;; *) exit 1;; esac
if [ ! -x "$bun_bin" ]; then
  asset="bun-$host-$arch$baseline"
  expected=$(sed -n "s/.*\"$asset.tgz\": *\"\([^\"]*\)\".*/\1/p" config/bun-runtime.json)
  mkdir -p .tools/bin
  temporary=$(mktemp -d "$project_root/.tools/bin/.bun-XXXXXX")
  trap 'rm -rf -- "$temporary"' EXIT HUP INT TERM
  curl -fL "https://registry.npmjs.org/@oven/$asset/-/$asset-$version.tgz" -o "$temporary/bun.tgz"
  if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$temporary/bun.tgz"); else actual=$(shasum -a 256 "$temporary/bun.tgz"); fi
  test "${actual%% *}" = "$expected"
  member="package/bin/${bun_bin##*/}"
  tar -xzf "$temporary/bun.tgz" -C "$temporary" "$member"
  executable="$temporary/$member"
  test "$("$executable" --version)" = "$version"
  mv "$executable" "$bun_bin"
fi
test "$("$bun_bin" --version)" = "$version"
"$bun_bin" install --frozen-lockfile
"$bun_bin" run proto:generate
