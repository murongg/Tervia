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
# hdiutil can underestimate sparse executables by their allocated blocks. Size the
# image from apparent bytes, with 20% plus 32 MiB for filesystem overhead.
TERVIA_SIZE_KIB="$(du -skA "$TERVIA_STAGE" | awk '{print $1}')"
TERVIA_SIZE_MIB=$(( (TERVIA_SIZE_KIB + 1023) / 1024 * 6 / 5 + 32 ))
hdiutil create -volname Tervia -fs HFS+ -size "${TERVIA_SIZE_MIB}m" \
  -srcfolder "$TERVIA_STAGE" -ov -format UDZO "build/Tervia-macos-$TERVIA_ARCH.dmg"
printf '%s\n' "$TERVIA_ROOT/build/Tervia-macos-$TERVIA_ARCH.zip" "$TERVIA_ROOT/build/Tervia-macos-$TERVIA_ARCH.dmg"
