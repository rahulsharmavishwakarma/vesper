#!/bin/sh
set -eu

version=${1:?release version is required}
test "$(uname -s)" = Darwin || {
  printf '%s\n' 'release packaging currently supports macOS only' >&2
  exit 1
}
architecture=$(uname -m)
case "$architecture" in
arm64) architecture=arm64 ;;
*)
  printf '%s\n' 'current release packaging targets Apple silicon' >&2
  exit 1
  ;;
esac

app=target/release/bundle/macos/transcribe.app
test -d "$app" || {
  printf '%s\n' "missing $app" >&2
  exit 1
}
bundle_version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")
test "$bundle_version" = "$version" || {
  printf '%s\n' 'bundle version does not match release version' >&2
  exit 1
}
executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
test -x "$app/Contents/MacOS/$executable" || {
  printf '%s\n' 'bundle executable is missing' >&2
  exit 1
}

dmg="target/release/bundle/dmg/transcribe_${version}_aarch64.dmg"
test -f "$dmg" || {
  printf '%s\n' "missing $dmg" >&2
  exit 1
}
hdiutil verify "$dmg"

output=release
test ! -e "$output" || {
  printf '%s\n' "$output already exists" >&2
  exit 1
}
mkdir "$output"
# The version lives in the tag and bundle; this name keeps the download URL stable.
cp "$dmg" "$output/transcribe-macos-${architecture}.dmg"
