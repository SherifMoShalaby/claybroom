#!/bin/sh
# Build Claybroom.app (Apple silicon + Intel) and pack it into dist/Claybroom-v<version>.dmg.
# Run from anywhere: sh packaging/macos-dmg.sh
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP=dist/Claybroom.app
STAGE=dist/dmg-stage
# Same naming as the Windows and Linux downloads: Claybroom-v<version>-…
DMG="dist/Claybroom-v$VERSION.dmg"

# One binary that runs on both kinds of Mac.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$APP" "$STAGE" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$STAGE"
lipo -create -output "$APP/Contents/MacOS/claybroom" \
  target/aarch64-apple-darwin/release/claybroom target/x86_64-apple-darwin/release/claybroom

# The icon, drawn by the app itself at every size macOS asks for.
ICONSET=dist/Claybroom.iconset
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  "$APP/Contents/MacOS/claybroom" --write-icon "$ICONSET/icon_${s}x${s}.png" "$s"
  "$APP/Contents/MacOS/claybroom" --write-icon "$ICONSET/icon_${s}x${s}@2x.png" "$((s * 2))"
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Claybroom.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Claybroom</string>
  <key>CFBundleDisplayName</key><string>Claybroom</string>
  <key>CFBundleIdentifier</key><string>io.github.claybroom</string>
  <key>CFBundleExecutable</key><string>claybroom</string>
  <key>CFBundleIconFile</key><string>Claybroom</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: enough for macOS to run it after "Open" in Finder's right-click menu.
# A Developer ID signature and notarisation (Apple account) would remove that step.
codesign --force --deep --sign - "$APP"

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Claybroom" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "$DMG"
