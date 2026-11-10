#!/usr/bin/env bash
# Builds the Claude Desktop extension: checks the bridge, runs its tests and
# packs manifest.json and server/ into slipcard-anki.mcpb (an MCPB bundle is
# a zip with manifest.json at its root). Needs Node 18 or newer and zip.
#
#   claude-desktop/build.sh [output directory, default claude-desktop/]
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
out=$(mkdir -p "${1:-$here}" && cd "${1:-$here}" && pwd)
bundle="$out/slipcard-anki.mcpb"
cd "$here"

node --check server/bridge.js
node -e 'const m = require("./manifest.json"); if (!m.name || !m.version || !m.server) process.exit(1);'
node --test test/bridge.test.js

rm -f "$bundle"
zip -X -q -r "$bundle" manifest.json server
echo "built: $bundle"
unzip -l "$bundle" | tail -n +2
