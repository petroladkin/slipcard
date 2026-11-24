#!/bin/bash
# Starts the headless Anki: the profile and the add-ons first, then a VNC
# display without a password, a window manager, and Anki, which is started
# again if it exits. On SIGTERM (docker stop) Anki is asked to close, so it
# saves the collection.
set -euo pipefail

PROFILE=${ANKI_PROFILE:-User 1}
[ -n "${PROFILE// }" ] || PROFILE="User 1"

python /opt/slipcard/configure.py

rm -f /tmp/.X99-lock /tmp/.X11-unix/X99
Xvnc :99 \
  -geometry "${VNC_GEOMETRY:-1600x1000}" -depth 24 \
  -rfbport 5900 -SecurityTypes None -AlwaysShared \
  -desktop Anki -nolisten tcp &
for _ in $(seq 1 50); do
  [ -e /tmp/.X11-unix/X99 ] && break
  sleep 0.2
done
openbox &

stopping=
anki_pid=
stop() {
  stopping=1
  if [ -n "$anki_pid" ]; then
    kill -TERM "$anki_pid" 2>/dev/null || true
  fi
}
trap stop TERM INT

while [ -z "$stopping" ]; do
  anki -b /data -p "$PROFILE" &
  anki_pid=$!
  # wait returns early when the trap runs; then wait for Anki to finish.
  wait "$anki_pid" || true
  if [ -n "$stopping" ]; then
    wait "$anki_pid" 2>/dev/null || true
    break
  fi
  echo "Anki exited; starting it again in 2 s"
  sleep 2
done
echo "Anki closed"
