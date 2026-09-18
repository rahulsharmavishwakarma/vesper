# Source this file from the repository root: source scripts/dev-env.sh
# Use the optional project-local toolchain without changing shell profiles.
if [ -x "$PWD/.tooling/cargo/bin/cargo" ]; then
  export CARGO_HOME="$PWD/.tooling/cargo"
  export RUSTUP_HOME="$PWD/.tooling/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
if [ -z "${TRANSCRIBE_MODEL_DIR:-}" ]; then
  if [ -n "${UKTI_MODEL_DIR:-}" ]; then
    export TRANSCRIBE_MODEL_DIR="$UKTI_MODEL_DIR"
  elif [ -n "${STT_MODEL_DIR:-}" ]; then
    export TRANSCRIBE_MODEL_DIR="$STT_MODEL_DIR"
  elif [ -d "$PWD/models/downloads" ]; then
    export TRANSCRIBE_MODEL_DIR="$PWD/models/downloads"
  fi
fi
if [ -x "$PWD/.tooling/cmake-4.4.3-macos-universal/CMake.app/Contents/bin/cmake" ]; then
  export PATH="$PWD/.tooling/cmake-4.4.3-macos-universal/CMake.app/Contents/bin:$PATH"
fi
# The native build verifies these libraries and embeds their patch/build identity.
if command -v node >/dev/null 2>&1; then
  TRANSCRIBE_SHERPA_LIBRARY_PATH=$(node scripts/sherpa-runtime.mjs --lib-dir)
  if [ -n "$TRANSCRIBE_SHERPA_LIBRARY_PATH" ]; then
    export SHERPA_ONNX_LIB_DIR="$TRANSCRIBE_SHERPA_LIBRARY_PATH"
  fi
  unset TRANSCRIBE_SHERPA_LIBRARY_PATH
fi
