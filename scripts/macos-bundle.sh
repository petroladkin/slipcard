#!/usr/bin/env bash
# Builds Slipcard.app for daily use on macOS: a release build, an accessory
# app (menu bar only, no Dock icon) with an ad-hoc signature. No Developer ID
# and no notarization: the bundle is for this Mac.
#
#   scripts/macos-bundle.sh                     build macos/.build/bundle/Slipcard.app
#   scripts/macos-bundle.sh --install           ... and install it into ~/Applications
#   scripts/macos-bundle.sh --install --dest D  ... into the folder D instead
#
# The version comes from linux/Cargo.toml (one version for both apps); the
# build number is the commit count. Installing replaces an existing
# Slipcard.app in the destination; the app is not started.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
install=no
dest="$HOME/Applications"
while [ $# -gt 0 ]; do
  case "$1" in
    --install) install=yes ;;
    --dest) dest=${2:?--dest needs a folder}; shift ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/linux/Cargo.toml" | head -1)
build=$(git -C "$root" rev-list --count HEAD 2>/dev/null || echo 0)
commit=$(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo unknown)

echo "== build (release) Slipcard $version ($build, $commit)"
cd "$root/macos"
swift build -c release --product Slipcard
bin="$(swift build -c release --show-bin-path)/Slipcard"

app="$root/macos/.build/bundle/Slipcard.app"
echo "== bundle $app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/Slipcard"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>io.github.petroladkin.Slipcard</string>
  <key>CFBundleName</key><string>Slipcard</string>
  <key>CFBundleDisplayName</key><string>Slipcard</string>
  <key>CFBundleExecutable</key><string>Slipcard</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$build</string>
  <key>SlipcardCommit</key><string>$commit</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.education</string>
  <!-- An accessory app: a menu bar item, no Dock icon, never activated by a popup. -->
  <key>LSUIElement</key><true/>
  <!-- Asked once at the first launch: AnkiConnect may run on another machine in the LAN. -->
  <key>NSLocalNetworkUsageDescription</key>
  <string>Slipcard talks to AnkiConnect on your local network to show your Anki cards and record your answers.</string>
  <!-- Plain http to local addresses only (an AnkiConnect in the LAN); https everywhere else. -->
  <key>NSAppTransportSecurity</key>
  <dict><key>NSAllowsLocalNetworking</key><true/></dict>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app"
codesign --verify --strict "$app"
echo "signed ad hoc: $(codesign -dv "$app" 2>&1 | sed -n 's/^Signature=//p')"

if [ "$install" = yes ]; then
  echo "== install into $dest"
  mkdir -p "$dest"
  target="$dest/Slipcard.app"
  if [ -e "$target" ]; then
    id=$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$target/Contents/Info.plist" 2>/dev/null || true)
    if [ "$id" != io.github.petroladkin.Slipcard ]; then
      echo "refusing: $target exists and is not Slipcard ($id)" >&2
      exit 1
    fi
    rm -rf "$target"
  fi
  ditto "$app" "$target"
  codesign --verify --strict "$target"
  echo "installed: $target (start it from Finder or Spotlight)"
fi
