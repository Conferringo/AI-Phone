#!/bin/sh
set -eu
. "$(dirname -- "$0")/android-env.sh"
out="$AI_PHONE_PROJECT_ROOT/data/android/network-helper"
mkdir -p "$out/classes" "$out/dex"
jar="$ANDROID_HOME/platforms/android-35/android.jar"
"$JAVA_HOME/bin/javac" --release 8 -classpath "$jar" -d "$out/classes" "$AI_PHONE_PROJECT_ROOT"/scripts/android-network/*.java
"$ANDROID_HOME/build-tools/35.0.0/d8" --lib "$jar" --min-api 26 --output "$out/dex" "$out/classes"/dev/aiphone/network/*.class
"$JAVA_HOME/bin/jar" cf "$out/network-proxy.jar" -C "$out/dex" classes.dex
