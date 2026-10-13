#!/bin/sh
# Disable tracing before reading any signing credentials, including when invoked with sh -x.
set +x
set -eu

fail() {
  printf 'macOS signing failed: %s\n' "$1" >&2
  exit 1
}

case "${1:-}" in
  -h|--help)
    printf '%s\n' 'Usage: sh scripts/sign.sh --check | /path/to/Tervia.app' \
      'Requires MACOS_CERTIFICATE, MACOS_CERTIFICATE_PASSWORD, APPLE_ID,' \
      'APPLE_TEAM_ID, and APPLE_APP_SPECIFIC_PASSWORD.'
    exit 0
    ;;
esac
[ "$#" -eq 1 ] || fail 'Pass --check or the path to the macOS app bundle.'
: "${MACOS_CERTIFICATE:?Set MACOS_CERTIFICATE to the Base64-encoded Developer ID Application p12.}"
: "${MACOS_CERTIFICATE_PASSWORD:?Set MACOS_CERTIFICATE_PASSWORD to the p12 export password.}"
: "${APPLE_ID:?Set APPLE_ID to the Apple developer account email.}"
: "${APPLE_TEAM_ID:?Set APPLE_TEAM_ID to the developer team ID.}"
: "${APPLE_APP_SPECIFIC_PASSWORD:?Set APPLE_APP_SPECIFIC_PASSWORD to an app-specific password.}"
[ "${#APPLE_TEAM_ID}" -eq 10 ] || fail 'APPLE_TEAM_ID must contain 10 uppercase letters or digits.'
case "$APPLE_TEAM_ID" in
  *[!A-Z0-9]*) fail 'APPLE_TEAM_ID must contain 10 uppercase letters or digits.' ;;
esac
if [ "$1" = --check ]; then
  printf '%s\n' 'macOS signing secrets are configured.'
  exit 0
fi
[ "$(uname -s)" = Darwin ] || fail 'Sign and notarize the app on macOS with Xcode installed.'
[ -d "$1/Contents/MacOS" ] && [ -f "$1/Contents/Info.plist" ] || fail 'The app bundle is missing its executable directory or Info.plist.'
sign_app=$(CDPATH= cd -- "$1" && pwd)

umask 077
sign_tmp=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/tervia-sign.XXXXXX")
sign_keychain=$sign_tmp/signing.keychain-db
sign_restore_search_list=false
set_search_list() {
  while IFS= read -r sign_existing_keychain; do
    set -- "$@" "$sign_existing_keychain"
  done < "$sign_tmp/search-list"
  security list-keychains -d user -s "$@"
}
cleanup() {
  sign_exit=$?
  trap - 0
  if [ "$sign_restore_search_list" = true ]; then
    if ! set_search_list; then
      printf '%s\n' 'Unable to restore the original keychain search list.' >&2
      sign_exit=1
    fi
  fi
  if [ -f "$sign_keychain" ]; then
    if ! security delete-keychain "$sign_keychain" >/dev/null 2>&1; then
      printf '%s\n' 'Unable to delete the temporary signing keychain.' >&2
      sign_exit=1
    fi
  fi
  if ! rm -rf -- "$sign_tmp"; then sign_exit=1; fi
  exit "$sign_exit"
}
trap cleanup 0
trap 'exit 130' INT
trap 'exit 143' TERM

printf '%s' "$MACOS_CERTIFICATE" | base64 --decode > "$sign_tmp/certificate.p12"
security list-keychains -d user > "$sign_tmp/search-list-raw"
sed -n 's/^[[:space:]]*"\(.*\)"$/\1/p' "$sign_tmp/search-list-raw" > "$sign_tmp/search-list"
sign_restore_search_list=true
sign_password=$(openssl rand -hex 32)
security create-keychain -p "$sign_password" "$sign_keychain"
# --keychain narrows identity selection; codesign still needs the search list.
set_search_list "$sign_keychain"
security set-keychain-settings -lut 21600 "$sign_keychain"
security unlock-keychain -p "$sign_password" "$sign_keychain"
security import "$sign_tmp/certificate.p12" -k "$sign_keychain" \
  -P "$MACOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
  -k "$sign_password" "$sign_keychain" >/dev/null
rm -f "$sign_tmp/certificate.p12"
unset MACOS_CERTIFICATE MACOS_CERTIFICATE_PASSWORD

# Select only the imported identity even when other keychains contain certificates.
security find-identity -v -p codesigning "$sign_keychain" > "$sign_tmp/identities"
sign_identity=$(awk -v team="$APPLE_TEAM_ID" '
  /"Developer ID Application: / && index($0, "(" team ")\"") { print $2 }
' "$sign_tmp/identities")
case "$sign_identity" in
  ''|*[!0-9A-Fa-f]*) fail 'Expected exactly one valid Developer ID Application identity matching APPLE_TEAM_ID.' ;;
esac
[ "${#sign_identity}" -eq 40 ] || fail 'Expected exactly one valid Developer ID Application identity matching APPLE_TEAM_ID.'

xcrun notarytool store-credentials tervia-notary --keychain "$sign_keychain" \
  --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
  --password "$APPLE_APP_SPECIFIC_PASSWORD" >/dev/null
unset APPLE_ID APPLE_APP_SPECIFIC_PASSWORD

printf '%s\n' 'Signing the macOS app with Developer ID...'
# Tervia contains one Rust executable. Do not use --deep to sign future nested code indiscriminately.
codesign --force --sign "$sign_identity" --keychain "$sign_keychain" \
  --options runtime --timestamp "$sign_app"
codesign --verify --deep --strict --verbose=2 "$sign_app"
ditto -c -k --sequesterRsrc --keepParent "$sign_app" "$sign_tmp/notarization.zip"

printf '%s\n' 'Submitting the app to Apple for notarization...'
sign_submit_exit=0
xcrun notarytool submit "$sign_tmp/notarization.zip" \
  --keychain-profile tervia-notary --keychain "$sign_keychain" \
  --wait --timeout 30m --output-format json > "$sign_tmp/result.json" || sign_submit_exit=$?
sign_status=$(plutil -extract status raw -o - "$sign_tmp/result.json" 2>/dev/null || true)
# A successful upload or timeout is not approval: require the final Accepted status.
if [ "$sign_submit_exit" -ne 0 ] || [ "$sign_status" != Accepted ]; then
  sign_submission=$(plutil -extract id raw -o - "$sign_tmp/result.json" 2>/dev/null || true)
  if [ -n "$sign_submission" ]; then
    if xcrun notarytool log "$sign_submission" --keychain-profile tervia-notary \
      --keychain "$sign_keychain" > "$sign_tmp/notary-log.json"; then
      cat "$sign_tmp/notary-log.json" >&2
    fi
  fi
  fail "Notarization was not accepted (status: ${sign_status:-unknown})."
fi

# Staple before the workflow builds its final archive, so offline installs carry the ticket.
xcrun stapler staple "$sign_app"
xcrun stapler validate "$sign_app"
spctl --assess --type execute --verbose=2 "$sign_app"
printf '%s\n' 'Developer ID signature, notarization ticket, and Gatekeeper checks passed.'
