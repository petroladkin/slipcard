#!/usr/bin/env bash
# Copies the newest of Anki's own backups out of the server's data directory
# and keeps the newest KEEP copies in the target directory. Run it nightly
# (cron or a systemd timer) on the host that has the data directory, with
# the target on another disk or machine.
#
#   server/backup.sh <data dir> <target dir> [keep, default 7]
#
# Anki writes a backup (a .colpkg with the whole collection) after the
# collection changes, at most every 30 minutes by default. A backup is put
# back with server/restore.sh (docs/server.md, "Import or restore").
set -euo pipefail

data=${1:?usage: backup.sh <data dir> <target dir> [keep]}
target=${2:?usage: backup.sh <data dir> <target dir> [keep]}
keep=${3:-7}

newest=$(find "$data" -mindepth 3 -maxdepth 3 -path '*/backups/*.colpkg' -printf '%T@ %p\n' \
  | sort -n | tail -n 1 | cut -d' ' -f2-)
if [ -z "$newest" ]; then
  echo "no Anki backup under $data" >&2
  exit 1
fi

profile=$(basename "$(dirname "$(dirname "$newest")")")
mkdir -p "$target"
name="${profile// /_}-$(basename "$newest")"
if [ -e "$target/$name" ]; then
  echo "already copied: $name"
else
  cp -p "$newest" "$target/$name.part"
  mv "$target/$name.part" "$target/$name"
  echo "copied: $name ($(du -h "$target/$name" | cut -f1))"
fi

# Keep the newest $keep copies (names sort by time within a profile).
find "$target" -maxdepth 1 -name '*.colpkg' -printf '%T@ %p\n' | sort -n \
  | head -n -"$keep" | cut -d' ' -f2- | while read -r old; do
  rm -f -- "$old"
  echo "removed: $(basename "$old")"
done
