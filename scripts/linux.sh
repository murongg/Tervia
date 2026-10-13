#!/usr/bin/env bash
set -euo pipefail
TERVIA_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TERVIA_ROOT"
if [[ "$(uname -s)" != Linux ]]; then
  printf '%s\n' 'Build this package on Linux.' >&2
  exit 1
fi
if [[ "${1:-}" != --no-build ]]; then cargo build --release --features native; fi
TERVIA_PACKAGE="$TERVIA_ROOT/build/linux/Tervia"
mkdir -p "$TERVIA_PACKAGE"
cp target/release/tervia "$TERVIA_PACKAGE/tervia"
cp assets/tervia.svg "$TERVIA_PACKAGE/tervia.svg"
cat assets/lobe-license.txt assets/hermes-license.txt > "$TERVIA_PACKAGE/ThirdPartyNotices.txt"
cat > "$TERVIA_PACKAGE/tervia.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Tervia
Exec=tervia
Icon=tervia
Terminal=false
Categories=Development;TerminalEmulator;
DESKTOP
tar -C build/linux -czf "build/Tervia-linux-$(uname -m).tar.gz" Tervia
printf '%s\n' "$TERVIA_ROOT/build/Tervia-linux-$(uname -m).tar.gz"
