#!/bin/sh
set -eu
folder=$1
core=$2
umask 077
mkdir -p "$folder"
echo $$ > "$folder/supervisor.pid"
child=''
finish(){
  if [ -n "$child" ]; then kill -TERM "$child" 2>/dev/null || true; wait "$child" 2>/dev/null || true; fi
  rm -f "$folder/supervisor.pid" "$folder/core.pid"
  exit 0
}
trap finish TERM INT
while [ -f "$folder/enabled" ] && [ -s "$folder/config.json" ]; do
  "$core" -d "$folder" -f "$folder/config.json" >/dev/null 2>&1 &
  child=$!
  echo "$child" > "$folder/core.pid"
  wait "$child" || true
  child=''
  sleep 2
done
