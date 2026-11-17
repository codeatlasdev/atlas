#!/usr/bin/env bash
# macos-keychain-import.sh <cert.p12> <keychain>
#
# Imports the Developer ID certificate of the release (the MACOS_CERTIFICATE_P12_BASE64 secret, decoded) into a new
# keychain that only codesign may read, for .github/workflows/release.yml. Passwords come from the environment, never
# argv: CERT_PASSWORD (the .p12's) and KEYCHAIN_PASSWORD (the new keychain's).
#
# `security import -P` has no stdin form, so the .p12 is re-encrypted under KEYCHAIN_PASSWORD first and only that
# throwaway password reaches a command line. The re-encrypted .p12 uses the legacy PKCS#12 algorithms (3DES, SHA-1 MAC):
# `security` cannot read what OpenSSL 3 writes by default (AES-256, SHA-256 MAC) and fails with "MAC verification failed
# during PKCS12 import (wrong password?)", which is what the first release run with the certificate hit (the runner's
# openssl is OpenSSL 3, not the system LibreSSL). Both OpenSSL 3 and LibreSSL accept the flags below.
set -euo pipefail
umask 077

die() { echo "macos-keychain-import: $*" >&2; exit 1; }

[ $# -eq 2 ] || die "usage: macos-keychain-import.sh <cert.p12> <keychain>"
p12=$1
kc=$2
[ -f "$p12" ] || die "certificate not found: $p12"
[ -n "${CERT_PASSWORD:-}" ] || die "CERT_PASSWORD is not set"
[ -n "${KEYCHAIN_PASSWORD:-}" ] || die "KEYCHAIN_PASSWORD is not set"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# LibreSSL cannot read the PEM stream back from a pipe, hence a 0600 file in a private directory.
openssl pkcs12 -in "$p12" -passin env:CERT_PASSWORD -nodes -out "$work/cert.pem" 2> "$work/err" \
	|| die "cannot open $p12 with CERT_PASSWORD: $(cat "$work/err")"
IMPORT_PW="$KEYCHAIN_PASSWORD" openssl pkcs12 -export -in "$work/cert.pem" -passout env:IMPORT_PW \
	-keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 -out "$work/import.p12"

security create-keychain -p "$KEYCHAIN_PASSWORD" "$kc"
security set-keychain-settings -lut 21600 "$kc"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$kc"
security import "$work/import.p12" -k "$kc" -P "$KEYCHAIN_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$kc" > /dev/null
