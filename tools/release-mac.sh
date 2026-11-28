#!/bin/sh
# Builds the Mac release: signed with a Developer ID, notarised by Apple, and with the
# files the in-app updater needs. Run from anywhere in the repository.
#
# Needs, on this Mac:
#   - a "Developer ID Application" certificate in the keychain
#   - notarisation credentials stored as a keychain profile:
#       xcrun notarytool store-credentials toona-notary --apple-id <email> --team-id <team>
#   - the update signing key at ~/.tauri/toona.key
#
# TOONA_SIGNING_IDENTITY and TOONA_NOTARY_PROFILE override the identity and profile used.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="${TOONA_NOTARY_PROFILE:-toona-notary}"
identity="${TOONA_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: .*\)"/\1/p' | head -1)}"
if [ -z "$identity" ]; then
  echo "No Developer ID Application certificate in the keychain." >&2
  exit 1
fi

cd "$root/app"
APPLE_SIGNING_IDENTITY="$identity" \
TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/toona.key")" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
  npx tauri build

bundle="$root/target/release/bundle"
dmg="$(ls -t "$bundle"/dmg/*.dmg | head -1)"
codesign --verify --deep --strict "$bundle/macos/Toona.app"

# Apple checks the disk image and the app inside it; the ticket covers both, and the
# copy of the app in the update file is the same signed app.
xcrun notarytool submit "$dmg" --keychain-profile "$profile" --wait
xcrun stapler staple "$dmg"
spctl --assess --type open --context context:primary-signature -v "$dmg"

echo "Notarised: $dmg"
