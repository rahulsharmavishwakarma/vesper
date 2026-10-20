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

product_name=$(node -p "require('./apps/desktop/src-tauri/tauri.conf.json').productName")
app="target/release/bundle/macos/$product_name.app"
test -d "$app" || {
  printf '%s\n' "missing $app" >&2
  exit 1
}
bundle_version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")
test "$bundle_version" = "$version" || {
  printf '%s\n' 'bundle version does not match release version' >&2
  exit 1
}
# A bundle whose signature does not seal its resources downloads as "damaged".
codesign --verify --deep --strict "$app" || {
  printf '%s\n' 'bundle signature is invalid or does not seal its resources' >&2
  exit 1
}
# The hardened runtime would make macOS refuse the ad hoc MLX library (#245).
if codesign -dvv "$app" 2>&1 | grep -q 'flags=.*runtime'; then
  printf '%s\n' 'app carries the hardened runtime flag; it cannot load the ad hoc MLX library' >&2
  exit 1
fi
executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
test -x "$app/Contents/MacOS/$executable" || {
  printf '%s\n' 'bundle executable is missing' >&2
  exit 1
}

dmg="target/release/bundle/dmg/${product_name}_${version}_aarch64.dmg"
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
cp "$dmg" "$output/vesper-macos-${architecture}.dmg"

# Updater artifacts exist only for enrolled (signed) builds. Unsigned
# candidates skip them; an archive without a signature fails closed so an
# unverifiable updater package can never publish.
updater_dir="target/release/bundle/macos"
updater_found=""
updater_count=0
for candidate in "$updater_dir"/*.app.tar.gz; do
  test -e "$candidate" || continue
  updater_count=$((updater_count + 1))
  updater_found="$candidate"
done
if [ "$updater_count" -eq 0 ]; then
  printf '%s\n' 'no updater archive; skipping updater artifacts (unsigned candidate)' >&2
elif [ "$updater_count" -gt 1 ]; then
  printf '%s\n' 'multiple updater archives; refusing to choose' >&2
  exit 1
elif [ ! -f "$updater_found.sig" ]; then
  printf '%s\n' "missing signature for $updater_found; refusing unsigned updater archive" >&2
  exit 1
else
  cp "$updater_found" "$output/vesper-macos-${architecture}.app.tar.gz"
  cp "$updater_found.sig" "$output/vesper-macos-${architecture}.app.tar.gz.sig"
fi
