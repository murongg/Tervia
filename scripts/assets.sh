#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 1 && "$1" =~ ^v[0-9][0-9A-Za-z.+-]*$ ]] || { printf '%s\n' 'Usage: bash scripts/assets.sh v<version> (from the collected assets directory)' >&2; exit 1; }
TERVIA_TAG=$1
shopt -s nullglob
TERVIA_ASSETS=()
for pattern in 'Tervia-macos-*.zip' 'Tervia-macos-*.dmg' 'Tervia-linux-*.tar.gz' 'Tervia-linux-*.deb' 'Tervia-linux-*.rpm' 'Tervia-windows-x64.zip' 'Tervia-windows-x64-setup.exe'; do
  matches=($pattern)
  if [[ ${#matches[@]} -ne 1 ]]; then
    printf 'Expected exactly one asset matching %s.\n' "$pattern" >&2
    exit 1
  fi
  TERVIA_ASSETS+=("${matches[0]}")
done
TERVIA_COLLECTED=(Tervia-*)
# Validate the complete set before renaming anything, so a missing format cannot publish a partial release.
[[ ${#TERVIA_COLLECTED[@]} -eq ${#TERVIA_ASSETS[@]} ]] || { printf '%s\n' 'Unexpected release assets.' >&2; exit 1; }
for asset in "${TERVIA_ASSETS[@]}"; do
  [[ -s "$asset" && -f "$asset" && ! -L "$asset" ]] || { printf 'Invalid release asset: %s\n' "$asset" >&2; exit 1; }
done
[[ -f "${TERVIA_ASSETS[0]%.zip}.dmg" && -f "${TERVIA_ASSETS[2]%.tar.gz}.deb" && -f "${TERVIA_ASSETS[2]%.tar.gz}.rpm" ]] || {
  printf '%s\n' 'Distribution formats must use the same architecture for each platform.' >&2
  exit 1
}
TERVIA_VERSIONED=()
for asset in "${TERVIA_ASSETS[@]}"; do
  destination="Tervia-$TERVIA_TAG-${asset#Tervia-}"
  mv "$asset" "$destination"
  TERVIA_VERSIONED+=("$destination")
done
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "${TERVIA_VERSIONED[@]}" > SHA256SUMS
else
  shasum -a 256 "${TERVIA_VERSIONED[@]}" > SHA256SUMS
fi
