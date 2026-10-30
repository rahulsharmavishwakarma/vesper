#!/bin/sh
# Imports the channel's self-signed code-signing certificate into a temporary
# keychain and hands it to Tauri. Every build of the channel then has the same
# designated requirement, so macOS keeps privacy permissions such as
# Accessibility across updates (#371). Without the secret, as on forks, the
# build stays ad hoc.
set -eu
: "${GITHUB_ENV:?}" "${RUNNER_TEMP:?}"

if [ -z "${MACOS_SIGNING_CERTIFICATE:-}" ]; then
  printf '%s\n' '::warning::No code-signing certificate: this build is signed ad hoc, so macOS asks for permissions again after updating to it.'
  exit 0
fi
test -n "${MACOS_SIGNING_CERTIFICATE_PASSWORD:-}" || {
  printf '%s\n' 'the certificate secret comes without its PKCS#12 passphrase secret' >&2
  exit 1
}

umask 077
keychain="$RUNNER_TEMP/vesper-signing.keychain-db"
p12="$RUNNER_TEMP/vesper-signing.p12"
password=$(openssl rand -hex 24)
printf '%s' "$MACOS_SIGNING_CERTIFICATE" | base64 --decode >"$p12"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$p12" -k "$keychain" -P "$MACOS_SIGNING_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null
rm -f "$p12"
security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" >/dev/null
# codesign looks the identity up in the user keychain search list.
# shellcheck disable=SC2046
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')

# A self-signed certificate is untrusted, so find-identity lists it without -v.
identity=$(security find-identity -p codesigning "$keychain" | awk '$1 == "1)" { print $2; exit }')
test -n "$identity" || {
  printf '%s\n' 'the certificate holds no code-signing identity' >&2
  exit 1
}
printf 'APPLE_SIGNING_IDENTITY=%s\nVESPER_SIGNING_IDENTITY=%s\nVESPER_SIGNING_KEYCHAIN=%s\n' \
  "$identity" "$identity" "$keychain" >>"$GITHUB_ENV"
printf 'Signing with certificate %s\n' "$identity"
