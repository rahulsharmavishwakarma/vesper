#!/bin/sh
# Creates the self-signed code-signing certificate for one release channel and
# stores it as that GitHub environment's secrets, so every build of the channel
# keeps the same identity and macOS keeps its permissions across updates
# (#371). The maintainer runs it; it prints only the public fingerprints.
#
#   sh scripts/create-signing-identity.sh production|nightly
#
# The identity is also imported into the login keychain as the backup. If it
# is lost, the next build has a new identity and users grant access once more.
set -eu

channel=${1:?usage: create-signing-identity.sh production|nightly}
case "$channel" in
production) name='Vesper Release Signing' ;;
nightly) name='Vesper Nightly Signing' ;;
*)
  printf '%s\n' 'channel must be production or nightly' >&2
  exit 1
  ;;
esac
repo=${VESPER_REPO:-rahulsharmavishwakarma/vesper}

if gh secret list --repo "$repo" --env "$channel" | grep -q '^MACOS_SIGNING_CERTIFICATE[[:space:]]'; then
  printf '%s\n' "$channel already has a signing certificate. Replacing it changes the app's identity, and users grant access again; delete the secret first to rotate on purpose." >&2
  exit 1
fi

umask 077
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat >"$work/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $name
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
EOF
# Twenty years: codesign refuses an expired certificate, and a new one would
# change the identity.
openssl req -x509 -newkey rsa:3072 -nodes -days 7300 -config "$work/cert.cnf" \
  -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
P12_PASSWORD=$(openssl rand -hex 24)
export P12_PASSWORD
openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" -name "$name" \
  -out "$work/identity.p12" -passout env:P12_PASSWORD

base64 <"$work/identity.p12" | tr -d '\n' |
  gh secret set MACOS_SIGNING_CERTIFICATE --repo "$repo" --env "$channel"
printf '%s' "$P12_PASSWORD" |
  gh secret set MACOS_SIGNING_CERTIFICATE_PASSWORD --repo "$repo" --env "$channel"
security import "$work/identity.p12" -k "$HOME/Library/Keychains/login.keychain-db" \
  -P "$P12_PASSWORD" -T /usr/bin/codesign >/dev/null

printf '%s\n' "Stored \"$name\" in the $channel environment and your login keychain."
openssl x509 -in "$work/cert.pem" -noout -fingerprint -sha1 | sed 's/^/Identity (leaf hash): /'
openssl x509 -in "$work/cert.pem" -noout -fingerprint -sha256 | sed 's/^/Certificate: /'
