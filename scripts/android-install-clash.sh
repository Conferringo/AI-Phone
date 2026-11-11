#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
exec "$native_host" --guest-app-resources "$AI_PHONE_PROJECT_ROOT" phone-install
