#!/bin/sh
# Checks a built bundle folder (target/<triple>/release/bundle): the app holds
# both architectures, its ad-hoc signature verifies strictly, and the DMG's
# checksum is valid. Prints the signing details for the release notes.
# Usage: scripts/verify-bundle.sh target/universal-apple-darwin/release/bundle
set -eu

bundle=${1:?usage: verify-bundle.sh <bundle folder>}
app="$bundle/macos/Forma.app"
binary="$app/Contents/MacOS/forma"

archs=$(lipo -archs "$binary")
echo "architectures: $archs"
case "$archs" in
  *x86_64*arm64* | *arm64*x86_64*) ;;
  *) echo "expected a universal binary (x86_64 and arm64)" >&2; exit 1 ;;
esac

codesign --verify --deep --strict --verbose=2 "$app"
codesign -dv "$app" 2>&1 | grep -E '^(Identifier|Format|CodeDirectory|Signature|TeamIdentifier)='

# Without a Developer ID signature and notarization, Gatekeeper rejects the
# app; that's expected and recorded, not a failure.
if spctl --assess --type execute "$app" 2>/dev/null; then
  echo "gatekeeper: accepted"
else
  echo "gatekeeper: rejected (not notarized; see docs/USER_GUIDE.md, Installing)"
fi

for dmg in "$bundle"/dmg/*.dmg; do
  [ -e "$dmg" ] || { echo "no DMG in $bundle/dmg" >&2; exit 1; }
  hdiutil verify -quiet "$dmg"
  echo "dmg: $(basename "$dmg") checksum valid"
done
