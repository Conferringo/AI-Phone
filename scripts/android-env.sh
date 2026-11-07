#!/bin/sh
# Source from project scripts; these variables apply only to this process tree.
AI_PHONE_PROJECT_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -x "$AI_PHONE_PROJECT_ROOT/runtime/services/bin/assistant" ]; then
  native_host="$AI_PHONE_PROJECT_ROOT/runtime/services/bin/assistant"
else
  native_host="$AI_PHONE_PROJECT_ROOT/target/debug/host-resources"
fi
runtime_exports=$("$native_host" --runtime-environment "$AI_PHONE_PROJECT_ROOT" "$HOME" shell)
eval "$runtime_exports"
ANDROID_HOME=$AI_PHONE_SDK_ROOT
JAVA_HOME=$AI_PHONE_JAVA_HOME
ANDROID_USER_HOME="$AI_PHONE_DATA_ROOT/android/user"
ANDROID_AVD_HOME="$AI_PHONE_DATA_ROOT/android/avd"
ANDROID_ADB_SERVER_PORT=15037
ADB_SERVER_SOCKET=tcp:127.0.0.1:15037
PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
if [ -n "$JAVA_HOME" ]; then PATH="$JAVA_HOME/bin:$PATH"; fi
export ANDROID_HOME JAVA_HOME ANDROID_USER_HOME ANDROID_AVD_HOME ANDROID_ADB_SERVER_PORT ADB_SERVER_SOCKET PATH

