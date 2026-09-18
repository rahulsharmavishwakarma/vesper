#!/bin/sh
set -eu
transcribe_source=$(node scripts/sherpa-runtime.mjs --source-dir)
if [ -z "$transcribe_source" ]; then
  echo 'Run make sherpa-runtime before native reporting tests.' >&2
  exit 1
fi
transcribe_test_dir=$(mktemp -d)
trap 'rm -rf "$transcribe_test_dir"' EXIT HUP INT TERM
"${CXX:-c++}" -std=c++17 -Wall -Wextra -Werror -I "$transcribe_source" \
  scripts/native-tests/decode-report.cpp -o "$transcribe_test_dir/decode-report"
"$transcribe_test_dir/decode-report"
