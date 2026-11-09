#!/usr/bin/env bash
# Sets up a headless Anki with AnkiConnect on an Ubuntu 24.04 host (a VM or
# a container), for development and integration tests. Run it as root:
#
#   ANKICONNECT_KEY=<key> [ANKI_PROFILE="Slipcard Test"] [ANKICONNECT_BIND=127.0.0.1] \
#     ./setup-headless-anki.sh
#
# What it does, idempotently:
#   1. installs the system libraries Qt needs, and Anki from PyPI (aqt,
#      pinned below) into a venv of a user "anki";
#   2. creates the profile without Anki's first-run dialogs;
#   3. installs the AnkiConnect add-on (2055492159) from AnkiWeb, with the
#      API key and the bind address;
#   4. installs and starts a systemd service "slipcard-test-anki" that runs
#      Anki on Qt's offscreen platform and restarts it if it stops.
#
# ANKICONNECT_BIND=0.0.0.0 makes AnkiConnect reachable from other hosts; do
# that only for a test profile, and only with a key. A request without the
# key is refused either way.
set -euo pipefail

AQT_VERSION=26.09.4
PROFILE=${ANKI_PROFILE:-Slipcard Test}
BIND=${ANKICONNECT_BIND:-127.0.0.1}
KEY=${ANKICONNECT_KEY:?set ANKICONNECT_KEY to the API key AnkiConnect will require}
USER_NAME=anki
HOME_DIR=/home/$USER_NAME
BASE=$HOME_DIR/anki-base
ADDON=$BASE/addons21/2055492159
UNIT=/etc/systemd/system/slipcard-test-anki.service

[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }

echo "== 1. packages and Anki $AQT_VERSION"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq python3-venv curl libegl1 libgl1 libxkbcommon0 libfontconfig1 \
  libdbus-1-3 libnss3 libasound2t64 libxcomposite1 libxdamage1 libxrandr2 libxtst6 \
  libxkbfile1 libxshmfence1 libgbm1 libdrm2 libxcb-cursor0 >/dev/null
id "$USER_NAME" >/dev/null 2>&1 || useradd -m -s /bin/bash "$USER_NAME"
su - "$USER_NAME" -c "python3 -m venv ~/venv && ~/venv/bin/pip install -q --upgrade pip \
  && ~/venv/bin/pip install -q aqt==$AQT_VERSION"

echo "== 2. profile \"$PROFILE\""
systemctl stop slipcard-test-anki.service 2>/dev/null || true
su - "$USER_NAME" -c "ANKI_PROFILE='$PROFILE' ~/venv/bin/python -" <<'PY'
import os
from pathlib import Path
from aqt.profiles import ProfileManager
name = os.environ["ANKI_PROFILE"]
base = Path.home() / "anki-base"
base.mkdir(exist_ok=True)
pm = ProfileManager(base)
pm.setupMeta()
# Without these Anki asks for a language and picks a profile on first run.
pm.meta["defaultLang"] = pm.meta.get("defaultLang") or "en_US"
pm.meta["firstRun"] = False
if name not in pm.db.list("select name from profiles where name != '_global'"):
    pm.create(name)
pm.load(name)
pm.set_last_loaded_profile_name(name)
pm.save()
print("profiles:", pm.profiles())
PY

echo "== 3. AnkiConnect"
tmp=$(mktemp -d)
# AnkiWeb picks the add-on build for the Anki version given as p: 26.09.4 -> 260904.
IFS=. read -r major minor patch <<<"$AQT_VERSION"
point=$(printf '%02d%02d%02d' "$((10#$major))" "$((10#$minor))" "$((10#${patch:-0}))")
curl -fsSL -o "$tmp/ac.zip" "https://ankiweb.net/shared/download/2055492159?v=2.1&p=$point"
echo "AnkiConnect package for p=$point, sha256: $(sha256sum "$tmp/ac.zip" | cut -d' ' -f1)"
rm -rf "$ADDON" && mkdir -p "$ADDON"
python3 -m zipfile -e "$tmp/ac.zip" "$ADDON"
rm -rf "$tmp"
BIND="$BIND" KEY="$KEY" python3 - "$ADDON/meta.json" <<'PY'
import json, os, sys
config = {"apiKey": os.environ["KEY"], "apiLogPath": None, "webBindAddress": os.environ["BIND"],
          "webBindPort": 8765, "webCorsOriginList": ["http://localhost"], "ignoreOriginList": []}
json.dump({"config": config}, open(sys.argv[1], "w"), indent=2)
PY
chown -R "$USER_NAME:$USER_NAME" "$BASE"

echo "== 4. systemd service"
cat > "$UNIT" <<EOF
[Unit]
Description=Headless Anki with AnkiConnect ($PROFILE)
After=network-online.target
Wants=network-online.target

[Service]
User=$USER_NAME
Environment=QT_QPA_PLATFORM=offscreen
Environment=QTWEBENGINE_CHROMIUM_FLAGS=--no-sandbox
ExecStart=$HOME_DIR/venv/bin/anki -b $BASE -p "$PROFILE"
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable -q --now slipcard-test-anki.service

for _ in $(seq 1 40); do
  if curl -s -m 2 127.0.0.1:8765 >/dev/null; then break; fi
  sleep 3
done
reply=$(curl -s -m 5 127.0.0.1:8765 -X POST \
  -d "{\"action\":\"getActiveProfile\",\"version\":6,\"key\":\"$KEY\"}")
echo "getActiveProfile: $reply"
case "$reply" in
  *"\"$PROFILE\""*) echo "ready: AnkiConnect on $BIND:8765" ;;
  *) echo "AnkiConnect did not answer as expected; see: journalctl -u slipcard-test-anki" >&2; exit 1 ;;
esac
