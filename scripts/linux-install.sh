#!/usr/bin/env bash
# Builds Slipcard for daily use on Linux and installs it for the current
# user: the release binary and a desktop entry, so the launcher (COSMIC,
# GNOME) lists it. Needs the build dependencies of docs/development.md.
#
#   scripts/linux-install.sh                  install into ~/.local
#   scripts/linux-install.sh --prefix DIR     into DIR/bin and DIR/share/applications
#   scripts/linux-install.sh --uninstall      remove what an install put there
#
# The app is not started, and "Start at login" is left to its tray menu.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
prefix="$HOME/.local"
mode=install
cargo=${CARGO:-cargo}
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix=${2:?--prefix needs a folder}; shift ;;
    --uninstall) mode=uninstall ;;
    -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

bin="$prefix/bin/slipcard"
desktop="$prefix/share/applications/io.github.petroladkin.Slipcard.desktop"

if [ "$mode" = uninstall ]; then
  rm -f "$bin" "$desktop"
  echo "removed: $bin $desktop"
  exit 0
fi

echo "== build (release)"
(cd "$root/linux" && "$cargo" build --release --locked)

echo "== install into $prefix"
install -Dm755 "$root/linux/target/release/slipcard" "$bin"
mkdir -p "$(dirname "$desktop")"
cat > "$desktop.tmp" <<EOF
[Desktop Entry]
Type=Application
Name=Slipcard
Comment=Anki vocabulary cards in a timed popup
Exec=$bin
Icon=accessories-dictionary
Terminal=false
Categories=Education;
StartupNotify=false
EOF
mv "$desktop.tmp" "$desktop"
# Launchers watch this folder; the entry has no MIME types, so no
# update-desktop-database is needed (and no cache is left behind).
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$desktop"; fi
echo "installed: $bin"
echo "launcher entry: $desktop"
case ":$PATH:" in *":$prefix/bin:"*) ;; *) echo "note: $prefix/bin is not on PATH (the launcher entry does not need it)" ;; esac
