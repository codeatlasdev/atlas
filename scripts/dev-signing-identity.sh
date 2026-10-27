#!/bin/sh
# Prints the identity that signs local debug bundles (see dev-signing-identity.test.ts for why): an explicit
# APPLE_SIGNING_IDENTITY, else the first "Apple Development" identity in the keychain, else "-" (ad hoc).
if [ -n "$APPLE_SIGNING_IDENTITY" ]; then
	echo "$APPLE_SIGNING_IDENTITY"
	exit 0
fi
id=$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Apple Development: [^"]*\)".*/\1/p' | head -1)
echo "${id:--}"
