#!/usr/bin/env bash
# Wraps the FolderCompare release binary into FolderCompare.app (runs on macOS: needs sips, iconutil, codesign).
# Usage: build/macos/make-app.sh <path to the FolderCompare binary> <version> <output dir>
set -euo pipefail

binary="$1"
version="$2"
out_dir="$3"
here="$(cd "$(dirname "$0")" && pwd)"
icon="$here/../../crates/foldercompare/assets/app-icon-256.png"

app="$out_dir/FolderCompare.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/FolderCompare"
chmod +x "$app/Contents/MacOS/FolderCompare"

# Icon: all iconset sizes scaled from the 256 px rendering of Assets/app-icon.svg (tools/make-icon.ps1).
iconset="$(mktemp -d)/FolderCompare.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/FolderCompare.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>FolderCompare</string>
  <key>CFBundleDisplayName</key><string>FolderCompare</string>
  <key>CFBundleIdentifier</key><string>io.github.darkcoder2000.foldercompare</string>
  <key>CFBundleExecutable</key><string>FolderCompare</string>
  <key>CFBundleIconFile</key><string>FolderCompare</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: required to run on Apple Silicon. Without a Developer ID, Gatekeeper still warns on first start.
codesign --force --deep --sign - "$app"
echo "Created $app"
