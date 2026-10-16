#!/bin/zsh
set -euo pipefail
TERVIA_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TERVIA_ROOT"

if [[ "${1:-}" != "--no-build" ]]; then
  cargo build --release --features native
fi

TERVIA_PACKAGE_ID="$(cargo pkgid --locked --offline)"
TERVIA_VERSION="${TERVIA_PACKAGE_ID##*@}"
# Apple bundle versions require the numeric core, without SemVer prerelease/build suffixes.
TERVIA_VERSION="${TERVIA_VERSION%%[-+]*}"

TERVIA_APP="$TERVIA_ROOT/build/Tervia.app"
mkdir -p "$TERVIA_APP/Contents/MacOS" "$TERVIA_APP/Contents/Resources" "$TERVIA_ROOT/build/Tervia.iconset"
cp "$TERVIA_ROOT/target/release/tervia" "$TERVIA_APP/Contents/MacOS/Tervia"
cat "$TERVIA_ROOT/assets/lobe-license.txt" "$TERVIA_ROOT/assets/hermes-license.txt" "$TERVIA_ROOT/assets/material-license.txt" > "$TERVIA_APP/Contents/Resources/ThirdPartyNotices.txt"

# Regenerate from the shared SVG so a cached PNG cannot hide a logo update.
swift "$TERVIA_ROOT/scripts/icon.swift" "$TERVIA_ROOT/build/icon.png"
for TERVIA_SIZE in 16 32 128 256 512; do
  sips -z "$TERVIA_SIZE" "$TERVIA_SIZE" "$TERVIA_ROOT/build/icon.png" --out "$TERVIA_ROOT/build/Tervia.iconset/icon_${TERVIA_SIZE}x${TERVIA_SIZE}.png" >/dev/null
  TERVIA_DOUBLE=$((TERVIA_SIZE * 2))
  sips -z "$TERVIA_DOUBLE" "$TERVIA_DOUBLE" "$TERVIA_ROOT/build/icon.png" --out "$TERVIA_ROOT/build/Tervia.iconset/icon_${TERVIA_SIZE}x${TERVIA_SIZE}@2x.png" >/dev/null
done
iconutil -c icns "$TERVIA_ROOT/build/Tervia.iconset" -o "$TERVIA_APP/Contents/Resources/Tervia.icns"

cat > "$TERVIA_APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Tervia</string>
<key>CFBundleDisplayName</key><string>Tervia</string>
<key>CFBundleIdentifier</key><string>dev.tervia.prototype</string>
<key>CFBundleExecutable</key><string>Tervia</string>
<key>CFBundleIconFile</key><string>Tervia</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>$TERVIA_VERSION</string>
<key>CFBundleShortVersionString</key><string>$TERVIA_VERSION</string>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleLocalizations</key><array>
<string>zh-Hans</string><string>zh-Hant</string><string>en</string>
<string>ja</string><string>ko</string><string>fr</string><string>de</string>
<string>es</string><string>pt</string><string>ru</string>
</array>
<key>LSMinimumSystemVersion</key><string>15.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$TERVIA_APP" >/dev/null 2>&1
printf '%s\n' "$TERVIA_APP"
