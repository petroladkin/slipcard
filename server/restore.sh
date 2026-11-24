#!/usr/bin/env bash
# Puts a collection package (.colpkg) on the server: your collection exported
# from a desktop Anki, or one of the server's own backups. It replaces the
# profile's collection; the current one is kept in the profile's backups
# folder as before-restore-<time>.anki2. Run it next to compose.yaml:
#
#   server/restore.sh <file.colpkg>
#
# Anki is stopped for the import and started again afterwards.
set -euo pipefail

package=${1:?usage: restore.sh <file.colpkg>}
[ -f "$package" ] || { echo "not a file: $package" >&2; exit 1; }
package=$(cd "$(dirname "$package")" && pwd)/$(basename "$package")
cd "$(dirname "$0")"

docker compose stop anki
docker compose run --rm --no-deps \
  -v "$package:/restore.colpkg:ro" \
  --entrypoint python anki /opt/slipcard/restore.py /restore.colpkg
docker compose start anki
echo "Anki started again"
