#!/usr/bin/env bash
set -euo pipefail
TERVIA_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TERVIA_ROOT"
[[ "$(uname -s)" == Darwin ]] || { printf '%s\n' 'Create macOS distributions on macOS.' >&2; exit 1; }
[[ -f build/Tervia.app/Contents/MacOS/Tervia ]] || { printf '%s\n' 'Run scripts/package.sh first.' >&2; exit 1; }
TERVIA_ARCH="$(uname -m)"
TERVIA_STAGE="$(mktemp -d "${TMPDIR:-/tmp}/tervia-dmg.XXXXXX")"
trap 'rm -rf -- "$TERVIA_STAGE"' EXIT

# Archive the already signed and stapled bundle; rebuilding here would invalidate notarization.
ditto -c -k --sequesterRsrc --keepParent build/Tervia.app "build/Tervia-macos-$TERVIA_ARCH.zip"
ditto build/Tervia.app "$TERVIA_STAGE/Tervia.app"
ln -s /Applications "$TERVIA_STAGE/Applications"
hdiutil create -volname Tervia -srcfolder "$TERVIA_STAGE" -ov -format UDZO "build/Tervia-macos-$TERVIA_ARCH.dmg"
printf '%s\n' "$TERVIA_ROOT/build/Tervia-macos-$TERVIA_ARCH.zip" "$TERVIA_ROOT/build/Tervia-macos-$TERVIA_ARCH.dmg"
