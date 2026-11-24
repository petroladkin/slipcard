#!/usr/bin/env bash
# Copies the newest of Anki's own backups of each profile out of the server's
# data directory and keeps the newest KEEP copies per profile in the target
# directory. Run it nightly (cron or a systemd timer) on the host that has
# the data directory, with the target on another disk or machine.
#
#   server/backup.sh <data dir> <target dir> [keep, default 7]
#
# Anki writes a backup (a .colpkg with the whole collection) after the
# collection changes, at most every 30 minutes by default. A backup is put
# back with server/restore.sh (docs/server.md, "Import or restore").
#
# A new copy is written as <name>.part and renamed when complete; older
# copies are removed only after that, so the target never holds zero copies
# of a profile, even with keep 1. Each profile is counted on its own, so a
# change in one profile never pushes out the copy of another.
set -euo pipefail

data=${1:?usage: backup.sh <data dir> <target dir> [keep]}
target=${2:?usage: backup.sh <data dir> <target dir> [keep]}
keep=${3:-7}
case $keep in
  '' | *[!0-9]* | 0) echo "keep must be a number of at least 1: $keep" >&2; exit 1 ;;
esac
[ -d "$data" ] || { echo "not a directory: $data" >&2; exit 1; }
mkdir -p "$target"

found=0
for backups in "$data"/*/backups; do
  [ -d "$backups" ] || continue
  newest=$(find "$backups" -mindepth 1 -maxdepth 1 -name '*.colpkg' -printf '%T@ %p\n' \
    | sort -n | tail -n 1 | cut -d' ' -f2-)
  [ -n "$newest" ] || continue
  found=1

  profile=$(basename "$(dirname "$backups")")
  prefix="${profile// /_}-"
  name="$prefix$(basename "$newest")"
  rm -f -- "$target/$prefix"*.colpkg.part
  if [ -e "$target/$name" ]; then
    echo "already copied: $name"
  else
    cp -p "$newest" "$target/$name.part"
    mv "$target/$name.part" "$target/$name"
    echo "copied: $name ($(du -h "$target/$name" | cut -f1))"
  fi

  # Keep this profile's newest $keep copies.
  find "$target" -maxdepth 1 -name "$prefix"'backup-*.colpkg' -printf '%T@ %p\n' | sort -n \
    | head -n -"$keep" | cut -d' ' -f2- | while read -r old; do
    rm -f -- "$old"
    echo "removed: $(basename "$old")"
  done
done

if [ "$found" = 0 ]; then
  echo "no Anki backup under $data" >&2
  exit 1
fi
