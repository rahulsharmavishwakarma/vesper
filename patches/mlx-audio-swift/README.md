# MLX Audio Swift qualification corrections

Applies only to MLX-Audio-Swift
`d302a5c6080d2bb97bae38c7418f82abb76013b6` (0.1.3).
`scripts/mlx-runtime.mjs` verifies the revision, applies the patch once and builds
against `native/mlx/Package.resolved` (MLX Swift 0.31.4, MLX Swift LM 3.31.4).

The patch exposes actual termination for bounded synchronous Cohere, Whisper,
Qwen3-ASR and Parakeet calls. Generative decoders distinguish EOS, limits,
repetition and cancellation. Parakeet reports complete frame consumption or
cancellation; its transducer does not use the generative token budget. Checks
between decoder steps allow cooperative cancellation; encoder/kernel calls remain
nonpreemptible. Fresh decoder state is still owned by the upstream implementation.

Whisper's local-directory factory cannot download a missing tokenizer. Qwen's
factory cannot silently generate one during inference. Prepared tokenizer bytes
must be verified before loading. An explicit `prepare-qwen-tokenizer` executable
uses the pinned upstream generator on verified local vocabulary/merge/config
files; its generated output must be hashed separately. The generator orders
added-token keys deterministically. It is never invoked by inference.

Whisper additionally reports SOT-row no-speech probability and greedy token
log probability, including EOS. Tokenizer validation accepts OpenAI's
`<|nospeech|>` and the Hugging Face `<|nocaptions|>` spelling. The Rust wrapper
reuses the application's existing combined confidence policy, after checking
completion, instead of accepting hallucinated silence output without scores.
See the pinned OpenAI
[decoder](https://github.com/openai/whisper/blob/v20250625/whisper/decoding.py) and
[transcription policy](https://github.com/openai/whisper/blob/v20250625/whisper/transcribe.py).

Qwen reuses the same module's existing `WhisperAudio.logMelSpectrogram` without
Whisper's 30-second padding. This matches its official Whisper frontend's Slaney
spacing, periodic Hann and removal of the last centered STFT frame. Its
audio-token length helper also uses floor division for complete 100-frame groups;
MLX `/` promotes integer arrays to floating point and previously overcounted short
final groups. Other architectures' preprocessing and Qwen's decoder policy remain
unchanged. References: [Qwen processor at 7c6daf77](https://github.com/QwenLM/Qwen3-ASR/blob/7c6daf77a2421100f5fb066495372c00129d39ff/qwen_asr/core/transformers_backend/processing_qwen3_asr.py),
and its pinned [Transformers 4.57.6 extractor](https://github.com/huggingface/transformers/blob/v4.57.6/src/transformers/models/whisper/feature_extraction_whisper.py).

`QwenFrontendTests` checks fixed official token-count expectations around encoder
chunk boundaries and the ten-second feature geometry. Run on a Metal-capable Mac
after the build script applies the patch:

```sh
swift test --package-path native/mlx --scratch-path .tooling/mlx-build \
  --cache-path .tooling/swift-cache --force-resolved-versions \
  -c release --filter QwenFrontendTests
```

The bridge pads very short input with trailing
zeros and caps each native call at 30 seconds; the application must retain original
sample accounting and use its existing architecture-specific segmentation.

This runtime-internal patch corrects Qwen preprocessing and enables the existing
application silence policy and completion/cancellation evidence. It adds no
attention kernel, new decoding algorithm or training change. A successful
build does not qualify any speech model. See
[inference performance](../../docs/inference-performance.md) for measured outcomes
and remaining integration gates. Reported MLX peak memory is process-lifetime,
including resident weights; it is not an incremental per-request allocation.

## Live Qwen session correction

The separate `live-session.patch` applies to the same pinned revision. It adds
an awaitable Qwen cancellation operation that joins decode/Stop tasks before
resetting mel/encoder state, bounds queued snapshot events to eight, and exposes
pending acoustic-window count. The C bridge admits at most two queued windows
and retains the resident model until its collector and native work have joined.
Default implementations for other upstream session cores do not imply equivalent
cancellation support; only Qwen is admitted by this bridge.

This patch does not establish final decoder completeness for native live text.
Live text remains provisional; Stop closes the native session and the application
uses the ordinary full-recording decode with strict termination checks. English
and Metal remain the qualified selection boundary. The build verifies exact
patched bytes and records hashes of both patches and both bridge sources. See
[live adapters](../../docs/live-transcription.md) for lifecycle and validation.

## Parakeet load hook

`parakeet-load-hook.patch` adds an optional `prepare` closure to
`ParakeetModel.fromDirectory`. The closure receives the constructed model and
its sanitized weights before strict verification. It may replace modules and
must return weights that match them. Stock Parakeet passes nil and loads
exactly as before.

VesperMLX uses the hook for Phonon-2 and Parakeet Redux. It replaces the 264
encoder projections with packed 2-bit layers: two planes per projection for
Phonon-2's five-value weights, one for Redux's ternary weights. Their float32
scales are taken from the fp16 file values before the loader casts float
parameters. The hook adds no kernel and does not change upstream decoding. See
[Phonon-2](../../docs/offline-asr.md#phonon-2-apple-gpu) and
[Parakeet Redux](../../docs/offline-asr.md#parakeet-redux-apple-gpu).

## Blocked TDT decoding

`parakeet-blocked-tdt.patch` adds a `.blocked` TDT decoder implementation and
makes it the default for single-recording decoding.

- It runs the same compiled step as the serial loop. The serial loop's
  reductions (duration jump, the zero-duration symbol cap and the state commit
  on non-blank tokens) move into the same compiled call.
- Frame index, symbol count, last token and LSTM state stay on the device.
- The host reads 16 step records per synchronization. Steps past the end are
  masked and leave the state unchanged.
- The serial and hybrid implementations remain selectable. Trace output is
  preserved.

Tokens, durations, timings and text match the serial loop exactly; see
`ParakeetBlockedDecodeTests` and
[inference performance](../../docs/inference-performance.md#blocked-tdt-decoding-on-metal--22-october-2026).
