#!/bin/sh
# Packages a Pondus release into dist/:
#   Pondus-<version>-macos-universal.zip   Pondus.app (x86_64 + arm64, with tigrim), ad-hoc signed
#   Pondus-<version>-windows-x64.zip       pondus.exe and tigrim.exe (x86_64-pc-windows-gnu)
# Needs the rustup targets aarch64-apple-darwin and x86_64-pc-windows-gnu, the MinGW-w64
# toolchain (x86_64-w64-mingw32-gcc) and macOS's lipo, iconutil, codesign and ditto.
set -e
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' app/Cargo.toml | head -1)
DIST=dist
mkdir -p "$DIST"

echo "== macOS x86_64 and arm64"
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
APP="$DIST/Pondus.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
for bin in pondus tigrim; do
  lipo -create -output "$APP/Contents/MacOS/$bin" "target/x86_64-apple-darwin/release/$bin" "target/aarch64-apple-darwin/release/$bin"
done
cp app/assets/Pondus.icns "$APP/Contents/Resources/Pondus.icns"
cp LICENSE NOTICE.md "$APP/Contents/Resources/"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Pondus</string>
  <key>CFBundleDisplayName</key><string>Pondus</string>
  <key>CFBundleIdentifier</key><string>com.github.sompote.pondus</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>pondus</string>
  <key>CFBundleIconFile</key><string>Pondus</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleDocumentTypes</key>
  <array><dict>
    <key>CFBundleTypeName</key><string>Pondus model</string>
    <key>CFBundleTypeExtensions</key><array><string>pondus</string></array>
    <key>CFBundleTypeRole</key><string>Editor</string>
  </dict></array>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>Copyright (C) 2026 Sompote Youwai. AGPL-3.0-only. Source: https://github.com/Sompote/pondus</string>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "$APP"
lipo -info "$APP/Contents/MacOS/pondus"
rm -f "$DIST/Pondus-$VERSION-macos-universal.zip"
(cd "$DIST" && ditto -c -k --sequesterRsrc --keepParent Pondus.app "Pondus-$VERSION-macos-universal.zip")

echo "== Windows x64"
CARGO_TARGET_DIR=target-win cargo build --release --target x86_64-pc-windows-gnu
WIN="$DIST/Pondus-$VERSION-windows-x64"
rm -rf "$WIN"
mkdir -p "$WIN"
cp target-win/x86_64-pc-windows-gnu/release/pondus.exe target-win/x86_64-pc-windows-gnu/release/tigrim.exe "$WIN/"
cp LICENSE NOTICE.md README.md "$WIN/"
rm -f "$WIN.zip"
(cd "$DIST" && zip -qr "Pondus-$VERSION-windows-x64.zip" "Pondus-$VERSION-windows-x64")
ls -la "$DIST"/*.zip
