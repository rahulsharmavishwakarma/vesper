#!/bin/sh
# Explicit installed-model validation. Never records audio or downloads weights.
set -eu

VESPER_PROJECT_DIR=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
cd "$VESPER_PROJECT_DIR"
. ./scripts/dev-env.sh

export CARGO_NET_OFFLINE=true
VESPER_SMOKE_MODELS="${VESPER_SMOKE_MODELS:-${TRANSCRIBE_SMOKE_MODELS:-${STT_SMOKE_MODELS:-${MODEL_DIR:-$VESPER_PROJECT_DIR/models/downloads}}}}"
if [ ! -d "$VESPER_SMOKE_MODELS" ]; then
  printf '%s\n' 'The installed model directory does not exist. Set MODEL_DIR or VESPER_SMOKE_MODELS.' >&2
  exit 1
fi
# Cargo runs tests from their crate directories; resolve every input first.
VESPER_SMOKE_MODELS=$(CDPATH='' cd "$VESPER_SMOKE_MODELS" && pwd)
export VESPER_SMOKE_MODELS
export VESPER_SMOKE_AUDIO="${VESPER_SMOKE_AUDIO:-${TRANSCRIBE_SMOKE_AUDIO:-${STT_SMOKE_AUDIO:-$VESPER_PROJECT_DIR/.cache/smoke/en.wav}}}"
export VESPER_SMOKE_GGUF="${VESPER_SMOKE_GGUF:-${TRANSCRIBE_SMOKE_GGUF:-${STT_SMOKE_GGUF:-$VESPER_SMOKE_MODELS/qwen3.5-0.8b/Qwen3.5-0.8B-Q4_0.gguf}}}"

if [ ! -s "$VESPER_SMOKE_AUDIO" ] || [ ! -s "$VESPER_SMOKE_GGUF" ]; then
  printf '%s\n' 'The public speech fixture and installed Qwen Q4 weights are required. See docs/validation.md; this command does not download files.' >&2
  exit 1
fi

absolute_file() {
  printf '%s/%s\n' "$(CDPATH='' cd "$(dirname "$1")" && pwd)" "$(basename "$1")"
}
VESPER_SMOKE_AUDIO=$(absolute_file "$VESPER_SMOKE_AUDIO")
VESPER_SMOKE_GGUF=$(absolute_file "$VESPER_SMOKE_GGUF")

if command -v shasum >/dev/null 2>&1; then
  VESPER_FIXTURE_DIGEST=$(shasum -a 256 "$VESPER_SMOKE_AUDIO")
elif command -v sha256sum >/dev/null 2>&1; then
  VESPER_FIXTURE_DIGEST=$(sha256sum "$VESPER_SMOKE_AUDIO")
else
  printf '%s\n' 'Install shasum or sha256sum to verify the public speech fixture.' >&2
  exit 1
fi
if [ "${VESPER_FIXTURE_DIGEST%% *}" != '148b936b43ce7c546a866e64da059f0458aee2d65e617f16e9d94f06e8d99ed6' ]; then
  printf '%s\n' 'Speech fixture checksum mismatch. Use the pinned public English fixture documented in docs/validation.md.' >&2
  exit 1
fi

# The desktop pipeline checks all selected model hashes before native loading.
cargo test --locked -p vesper-desktop \
  runtime::tests::real_file_pipeline_preserves_audio_predictions_and_export_provenance \
  -- --ignored --exact --nocapture
cargo test --locked -p vesper-inference \
  manager::tests::real_nemotron_stream_reuses_weights_and_resets_utterance_state \
  -- --ignored --exact --nocapture
cargo test --locked -p vesper-inference \
  qwen::tests::existing_gguf_embedded_template_formats_messages_and_rejects_role_tokens \
  -- --ignored --exact --nocapture
