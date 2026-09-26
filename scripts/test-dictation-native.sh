#!/bin/sh
# Native UI smoke; opt-in because it briefly presents a test panel.
set -eu
[ "$(uname -s)" = Darwin ] || {
  echo 'Native dictation smoke requires macOS.'
  exit 1
}
probe_dir=$(mktemp -d "${TMPDIR:-/tmp}/vesper-dictation.XXXXXX")
trap 'rm -rf "$probe_dir"' EXIT HUP INT TERM
xcrun clang -fobjc-arc -fblocks -framework AppKit -framework ApplicationServices \
  -framework Carbon crates/vesper-desktop-platform/tests/paste_accessibility.m \
  -o "$probe_dir/paste-accessibility"
"$probe_dir/paste-accessibility"
xcrun clang -fobjc-arc -fblocks -framework AppKit -framework ApplicationServices \
  -framework Carbon -framework WebKit crates/vesper-desktop-platform/tests/macos_native.m \
  -o "$probe_dir/native-smoke"
"$probe_dir/native-smoke"
