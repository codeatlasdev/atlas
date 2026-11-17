#!/usr/bin/env bash
# macos-signing-secrets.sh <Developer-ID.cer> [--repo owner/name] [--dry-run]
#
# Turns the certificate Apple issued (the .cer the Account Holder downloads after uploading ~/.config/stores/devid/devid.csr)
# into the repository secrets that .github/workflows/release.yml reads to sign and notarize the macOS build
# (docs/release.md, "macOS signing and notarization"):
#
#   MACOS_CERTIFICATE_P12_BASE64   the certificate + its private key (devid.key) as a .p12, base64
#   MACOS_CERTIFICATE_PASSWORD     a random password made here for that .p12 (it exists nowhere else)
#   MACOS_SIGNING_IDENTITY         "Developer ID Application: CODEATLAS LTDA (58UYCA3ZYA)", read from the certificate
#   APPLE_API_KEY_ID               ASC_KEY_ID of ~/.config/stores/apple.env  (the App Store Connect API key of the mobile
#   APPLE_API_ISSUER               ASC_ISSUER_ID                              release, reused by `notarytool`)
#   APPLE_API_KEY_P8               the file ASC_KEY_PATH points to
#
# No value is ever printed or put on a command line: the password and the keys travel through shell builtins, openssl's
# env: arguments and `gh secret set`'s stdin. --dry-run does every check (files, the certificate matches the key, the
# .p12 is built and read back) and prints only the secret names; it calls no gh. Run it first.
#
# Overrides, for tests: DEVID_KEY (default ~/.config/stores/devid/devid.key), APPLE_ENV_FILE (default
# ~/.config/stores/apple.env), GH (default gh).
set -euo pipefail
umask 077

die() { echo "macos-signing-secrets: $*" >&2; exit 1; }

cer=""
repo="codeatlasdev/atlas"
dry=0
while [ $# -gt 0 ]; do
	case $1 in
		--dry-run) dry=1 ;;
		--repo) [ $# -ge 2 ] || die "--repo needs owner/name"; repo=$2; shift ;;
		-h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
		-*) die "unknown option $1" ;;
		*) [ -z "$cer" ] || die "one certificate only"; cer=$1 ;;
	esac
	shift
done
[ -n "$cer" ] || die "usage: macos-signing-secrets.sh <Developer-ID.cer> [--repo owner/name] [--dry-run]"

key=${DEVID_KEY:-$HOME/.config/stores/devid/devid.key}
apple_env=${APPLE_ENV_FILE:-$HOME/.config/stores/apple.env}
gh=${GH:-gh}

[ -f "$cer" ] || die "certificate not found: $cer"
[ -f "$key" ] || die "private key not found: $key (the key of the CSR the certificate was made from stays on the Mac that made the CSR)"
[ -f "$apple_env" ] || die "notarization credentials not found: $apple_env"
command -v openssl > /dev/null || die "openssl is missing"
[ "$dry" = 1 ] || command -v "$gh" > /dev/null || die "gh is missing"

# KEY=value lines, read without sourcing (the file is not trusted to be shell); optional quotes are dropped.
envval() { sed -n "s/^$1=//p" "$apple_env" | head -n 1 | tr -d "\"'"; }
key_id=$(envval ASC_KEY_ID)
issuer=$(envval ASC_ISSUER_ID)
p8_path=$(envval ASC_KEY_PATH)
# The file is written for a shell: a path may start with ~ or $HOME / ${HOME}.
p8_path=${p8_path/#\~/$HOME}
p8_path=${p8_path/#\$HOME/$HOME}
p8_path=${p8_path/#\$\{HOME\}/$HOME}
[ -n "$key_id" ] || die "ASC_KEY_ID is not set in $apple_env"
[ -n "$issuer" ] || die "ASC_ISSUER_ID is not set in $apple_env"
[ -n "$p8_path" ] || die "ASC_KEY_PATH is not set in $apple_env"
[ -f "$p8_path" ] || die "the ASC_KEY_PATH file does not exist: $p8_path"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Apple hands out DER; accept PEM as well.
if ! openssl x509 -inform DER -in "$cer" -out "$work/cert.pem" 2> /dev/null; then
	openssl x509 -inform PEM -in "$cer" -out "$work/cert.pem" 2> /dev/null || die "$cer is not an X.509 certificate"
fi

identity=$(openssl x509 -in "$work/cert.pem" -noout -subject -nameopt multiline,utf8 | sed -n 's/^ *commonName *= *//p')
case $identity in
	"Developer ID Application: "*) ;;
	*) die "this is not a Developer ID Application certificate (its name is \"$identity\"); Apple's other types cannot be notarized" ;;
esac
openssl x509 -in "$work/cert.pem" -noout -checkend 0 > /dev/null || die "the certificate has expired"

cert_pub=$(openssl x509 -in "$work/cert.pem" -noout -pubkey)
key_pub=$(openssl pkey -in "$key" -pubout 2> /dev/null) || die "$key is not a readable private key (is it passphrase protected?)"
[ "$cert_pub" = "$key_pub" ] || die "the certificate was not made from $key (public keys differ): it came from another CSR"

# 3DES + SHA-1 MAC: what macOS `security import` (and the runner's LibreSSL) read; OpenSSL 3's AES default they do not.
password=$(openssl rand -base64 33)
export P12_PASSWORD=$password
openssl pkcs12 -export -inkey "$key" -in "$work/cert.pem" -name "$identity" \
	-certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 -passout env:P12_PASSWORD -out "$work/devid.p12"
openssl pkcs12 -in "$work/devid.p12" -passin env:P12_PASSWORD -nokeys -noout 2> /dev/null || die "the .p12 does not read back"

names="MACOS_CERTIFICATE_P12_BASE64 MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY APPLE_API_KEY_ID APPLE_API_ISSUER APPLE_API_KEY_P8"
if [ "$dry" = 1 ]; then
	echo "dry run: would set on $repo:"
	for n in $names; do echo "  $n"; done
	exit 0
fi

# `gh secret set NAME` reads the value from stdin; printf is a builtin, so no value reaches a command line.
set_secret() { "$gh" secret set "$1" --repo "$repo" > /dev/null || die "could not set $1"; echo "set $1"; }
base64 < "$work/devid.p12" | tr -d '\n' | set_secret MACOS_CERTIFICATE_P12_BASE64
printf '%s' "$password" | set_secret MACOS_CERTIFICATE_PASSWORD
printf '%s' "$identity" | set_secret MACOS_SIGNING_IDENTITY
printf '%s' "$key_id" | set_secret APPLE_API_KEY_ID
printf '%s' "$issuer" | set_secret APPLE_API_ISSUER
set_secret APPLE_API_KEY_P8 < "$p8_path"
