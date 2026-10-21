# Qwen3-ASR and Cohere

The application has dedicated CPU adapters for Qwen3-ASR 0.6B and Cohere Transcribe. Select them in **Models**, choose a compatible transcription language in **Dictate** or **Settings**, then use **Save & download**. The language menu lists only modes supported by the active adapter; it guides recognition and does not translate the transcript. Selecting a model alone does not download it. They process completed recordings and imported files without an application duration cutoff. Long audio is processed internally in bounded parts; see [ADR-0011](adrs/0011-segment-long-generative-asr.md). The old recording-duration preference is ignored.

| Model | App language choices | Pinned download |
| --- | --- | ---: |
| Qwen3-ASR 0.6B INT8 | Automatic plus 30 declared languages | 987,015,347 bytes |
| Cohere Transcribe INT8 | 14 declared languages, explicit only | 2,888,052,036 bytes |

Cohere has no automatic language detection or Hindi support in this adapter. The picker follows `models/languages.json`. Both adapters accept Automatic/CPU compute settings. Qwen3-ASR is a speech model, separate from the Qwen cleanup model. Their files cannot be imported as a four-file NeMo transducer profile.

## Completion contract

Sherpa 1.13.8 exposes both native model configurations, but its ordinary result omits the reason decoding stopped. The application maintains a small reporting patch against upstream commit `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`; approval and scope are recorded in [issue #14](https://github.com/rahulsharmavishwakarma/vesper/issues/14).

The native patch publishes `vesper.decode_report.v1` using the existing per-stream option API. It reports `schemaVersion`, `stopReason`, and `inputContextTruncated`. The completed-audio path accepts text only after actual EOS with no discarded input context. Output/context caps, repetition termination, numerical/token errors, unknown versions, and missing reports reject the result. A native silence/no-input result is accepted only with empty text. Every decode clears the previous report/result. Qwen-ASR and Cohere now retain verified recognizers in the application worker. [Live previews](live-transcription.md) reuse those weights with fresh decode state; the final completed-audio pass remains authoritative. Cohere residency was introduced under [ADR-0017](adrs/0017-resident-cohere-recognizer.md), verifying artifacts on load and invalidating on observed file/options changes. Every segment still gets a fresh stream; cancellation is checked before/after native calls, which cannot be interrupted mid-decode.

A live preview has a separate provisional contract: a valid duration-token-limit
report may expose a replaceable draft, but cannot satisfy completed-result
validation. Context loss, missing/unknown reports and repetition/error termination
remain failures. The final pass still uses the strict contract above.

The project-owned patch headers, option keys and `vesper-runtime.json` build
manifest use the Vesper namespace. This requires a genuine rebuild: source,
patch and library hashes must agree; renaming a cached manifest or binary is
not sufficient. Upstream `cohere-transcribe` architecture names are unchanged.
Whisper confidence/language options use `vesper.whisper_scores.v1` and
`vesper.whisper_language.v1`. Rust embeds verified provenance as
`VESPER_SHERPA_BUILD_ID` and `VESPER_SHERPA_PATCH_SHA256`; these are build-script
outputs, not operator overrides. Runtime reports keep the same JSON schema.

Completion is **not an accuracy guarantee**. A model can reach EOS while misrecognizing words. The Qwen export generated a sentence from an all-zero one-second WAV during qualification. The app therefore returns empty text for exact digital silence before model loading, after validating the sample rate and configuration. Exact-zero segments inside longer audio are skipped too. This is an exact-zero check, not a volume threshold, VAD, or segmentation algorithm; nonzero quiet/noisy audio still needs model-quality review.

The patch records native control flow without changing normal sampling or weights. Qwen keeps its total context allowance of 512 tokens and sets its output budget to the same value, removing the earlier independent 128-token ceiling. The native context bound and completion checks still reject exhaustion. This existing public configuration does not enlarge the preallocated KV cache; longer generations can take more CPU time and delay cancellation. Cohere retains its upstream duration/model output cap, punctuation enabled, and inverse text normalization disabled. Inputs and incomplete text are never silently shortened by application code to make a test pass. Original audio remains in the existing archive when collection is enabled.

## Long audio

The adapter balances the remaining PCM into parts of at most ten seconds, then prefers a quieter boundary within the preceding second. It neither drops audio nor overlaps ranges. Each part uses a fresh native stream and must pass the completion check. Text is joined in chronological order without removing genuine repetitions. A failed or cancelled part prevents publication of a successful prefix; the original audio remains available when saving is enabled.

Dictate and Quick Capture show completed-audio progress. A running native call remains noninterruptible. Words at segment boundaries can be misrecognized, and each part has independent language/context state. Existing memory and writing-context limits still apply; a failed writing pass retains the full raw transcript. See [long-input validation](validation.md#long-qwen3-asr-and-cohere-audio) for measurements and limitations.

## Build and provenance

Run `make sherpa-runtime`, then `source scripts/dev-env.sh` before direct Cargo commands. `make setup` also builds the runtime. The helper downloads a SHA-256-pinned source archive, applies [the reporting patch](../patches/sherpa-onnx/decode-report.patch), and runs the pinned upstream CMake dependency recipes. No model weights are downloaded by build or unit-test commands.

The helper uses separate build directories derived from source/patch/compiler/platform/flags. Its manifest hashes the 13 required static libraries. `vesper-inference/build.rs` checks those hashes and the current patch before compiling; the executable records build and patch IDs in inference provenance. `SHERPA_ONNX_LIB_DIR` selects this build through the existing binding hook. Cargo registry sources and the upstream prebuilt cache are not modified. Generated sources, libraries, and weights remain ignored.

The source-build helper targets macOS/Linux arm64/x64; local model qualification is on macOS arm64. CI builds macOS and Linux and tests the report without weights. Other targets, including Windows, need a corresponding patched-runtime build and qualification before these adapters can be offered there. Published benchmark numbers do not establish performance on those machines.

## Artifacts and attribution

All ten required files, including Cohere's adjacent `encoder.int8.onnx.data` and Qwen's tokenizer directory, are pinned individually in [artifacts.json](../models/artifacts.json). The normal installer validates byte size and SHA-256, prevents path escapes, and installs files atomically. The job path verifies artifacts before inference.

- [Qwen export](https://huggingface.co/csukuangfj2/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25/tree/68818b2313fe77bd06f6a7c5068ff3ef59d02b8a), revision `68818b2313fe77bd06f6a7c5068ff3ef59d02b8a`.
- [Cohere export](https://huggingface.co/csukuangfj2/sherpa-onnx-cohere-transcribe-14-lang-int8-2026-04-01/tree/156a470cf08eefe706a0004f3c52d9ee567ca7a0), revision `156a470cf08eefe706a0004f3c52d9ee567ca7a0`.

Original [Qwen](https://huggingface.co/Qwen/Qwen3-ASR-0.6B) and [Cohere](https://huggingface.co/CohereLabs/cohere-transcribe-03-2026) metadata declare Apache-2.0. The export cards declare derivation but do not provide separate license declarations or exact original-checkpoint/conversion-build mappings. Those limitations are retained explicitly; artifact integrity is distinct from conversion reproducibility. No gated original weights or exporter code are bundled.

## Qualification

Current long-audio results, performance definitions and accuracy limitations are in [validation](validation.md#long-qwen3-asr-and-cohere-audio). Detailed evidence remains locally at `.cache/benchmarks/results/long-asr/debug-summary.json`. The earlier [short-clip integration qualification](validation.md#qwen3-asr-and-cohere-integration) is retained as historical evidence.

`make check` includes ordinary Rust/JS checks and `make test-native-report`. The report tests cover reset/isolation, retained context-loss flags, EOS at an output boundary, and invalid/masked numerical rows. Rust tests cover rejected stop reasons, language mapping, sample coverage and tiny-tail avoidance, failure/cancellation between segments, cancellation, settings round trips, file layout and exact download selection. Browser tests exercise save-before-download, Cohere's language error, Qwen Hindi, the absence of duration restrictions, and processing progress.

Full model tests are opt-in and never download files:

```sh
source scripts/dev-env.sh
# VESPER_MODEL_DIR defaults to models/downloads; point it at installed artifacts.
VESPER_ASR_FIXTURES=/path/to/verified-fixtures cargo test --locked \
  -p vesper-inference qualify_qwen3_asr_public_fixtures -- --ignored --nocapture
VESPER_ASR_FIXTURES=/path/to/verified-fixtures cargo test --locked \
  -p vesper-inference qualify_cohere_public_fixtures -- --ignored --nocapture
VESPER_ASR_FIXTURES=/path/to/verified-fixtures cargo test --locked \
  -p vesper-inference qualify_qwen_long_audio -- --ignored --nocapture
VESPER_ASR_FIXTURES=/path/to/verified-fixtures cargo test --locked \
  -p vesper-inference qualify_cohere_long_audio -- --ignored --nocapture
```

The public fixtures are original Google FLEURS test audio, revision `70bb2e84b976b7e960aa89f1c648e09c59f894dd`, **CC-BY-4.0**, attributed to Google and Conneau et al., [FLEURS (2022)](https://arxiv.org/abs/2205.12446). References come from each language's `test.tsv`, not model output. See the [pinned dataset](https://huggingface.co/datasets/google/fleurs/tree/70bb2e84b976b7e960aa89f1c648e09c59f894dd).

| Local fixture name | Original archive member | Seconds | SHA-256 |
| --- | --- | ---: | --- |
| `fleurs-en_us.wav` | `test/10052240106321793346.wav` | 8.76 | `7603cae3294309ef70d234180cefccc3b13865ee525262228a636069b9b62841` |
| `fleurs-fr_fr.wav` | `test/10138021779681596478.wav` | 8.28 | `05ccf3ccc2ab4b72d056c3f02e3ffcf6ea0f37b1c42bea5c35789c85291fbdc5` |
| `fleurs-hi_in.wav` | `test/10011266027513218401.wav` | 9.12 | `217f3abbd8e9cd3767d6e8b87d49fbf82421fc920a103de026b4586d7e644dc5` |

Synthetic mono 16 kHz all-zero WAVs cover one second, exactly ten seconds, and 160,001 samples (one sample beyond ten seconds). These few fixtures are smoke checks; they do not constitute a language benchmark or prove quality on general dictation. The live tests also force Qwen output/context limits and reuse a stream after success to verify failure does not retain an earlier completion report.

To qualify archival/provenance and the post-stop backend path using the same public PCM (without opening the microphone):

```sh
VESPER_ASR_FIXTURES=/path/to/verified-fixtures cargo test --locked \
  -p vesper-desktop new_offline_asr_pipeline_preserves_audio_and_provenance \
  -- --ignored --nocapture
```

The long qualifiers repeat each original fixture three times, then repeat with two seconds of leading digital silence to shift boundaries. They use the production adapter, check cancellation after the first part, and compare fresh A/B/A streams and failure recovery on the same recognizer. The desktop test imports and processes a 26.28-second three-copy English WAV through both file and post-stop paths, asserting original-byte/PCM identity and provenance. The local long-audio debug report additionally records natural FLEURS members, full references, fixture hashes, and MUCS attribution for the benchmark manifests.

The standalone [native probe](../scripts/native-tests/asr-probe.cpp) is retained for historical short-clip patch-parity checks; use the Rust qualifiers above for the current segmented application path. It can link against either the original upstream libraries or the patched libraries. On macOS, after sourcing the development environment:

```sh
c++ -std=c++17 -O2 -I "$(node scripts/sherpa-runtime.mjs --source-dir)" \
  scripts/native-tests/asr-probe.cpp "$SHERPA_ONNX_LIB_DIR"/lib*.a \
  -framework Foundation -o /tmp/vesper-asr-probe
/usr/bin/time -l /tmp/vesper-asr-probe qwen "$VESPER_MODEL_DIR" \
  /path/to/verified-fixtures/fleurs-en_us.wav English
```

Use `cohere`/`en` or `parakeet`/`en` for the other adapters. Link the same probe against the original 1.13.8 static library directory to compare normal results. Run measurements sequentially; the probe does not perform the installer's hash verification, download files, or use the application's result gate. Its report is null with the original runtime and for unchanged Parakeet.

The non-silent boundary fixture `fleurs-en_us-padded-10s.wav` retains every original English float32 sample and appends 19,840 zero samples (1.24 seconds). Its canonical minimal RIFF/WAVE header declares format 3, one channel, 16,000 Hz, 64,000 bytes/second, four-byte block alignment and 32-bit samples, followed by a 640,000-byte data chunk. File size is 640,044 bytes; SHA-256 is `994f93a06e1e5d45b2b3bdb3e219b39e1976cba413a139b96675cc12c7f1411d`. This is an explicitly modified CC-BY-4.0 FLEURS derivative with added trailing silence, retaining the English reference and attribution above.

## Moonshine Base (English)

Moonshine Base runs through a dedicated `MoonshineAsr` adapter on sherpa-onnx's
Moonshine v2 recognizer (`encoder_model.ort` plus `decoder_model_merged.ort`).

- **Following the reference runtime:**
  - Completed audio is decoded in parts of at most 15 s.
  - The native token budget is 6.5 tokens per second of audio. Reaching it
    reports `duration_token_limit` and the result is rejected.
- **Pinned-runtime fix:** the patch backports sherpa-onnx's raw-length decoder
  mask fix (#3976). Without it, every decode above about 9.2 s failed silently.
- **Empty-result retry:** a non-silent part of at least 2 s that decodes to
  nothing at EOS is decoded once more as two halves, split at the quietest
  point near the middle.
- **Scope:** English only; never a default.

Held-out FLEURS en-US WER is 10.11% at a median of 86 ms per clip on CPU. See
the [adapter record](validation/moonshine-base-2026-10-21.md).

## Granite Speech 5.0 TurboCTC (Apple GPU)

Granite runs in Vesper's own Swift port inside the MLX bridge
(`GraniteSpeech5CTC.swift`, architecture `granite_speech5_ctc`). It uses IBM's
unmodified BF16 files. It is not a sherpa-onnx model and has no CPU path.

- **Following the reference:**
  - IBM's log-mel and delta frontend with two-frame stacking.
  - 128-frame block attention with Shaw relative positions.
  - Two stride-2 convolution layers and middle-layer self-conditioning.
  - Greedy CTC with a byte-fallback BPE detokenizer.
- **Parts of at most 30 s:** attention never spans more than 1.28 s, so parts
  lose no context.
- **Input leveling** (a Vesper adaptation): quiet input is boosted toward
  −23 dBFS RMS, capped at +40 dB and a 0.9 peak. The features have no
  per-utterance normalization, and this took the −30 dB lane from 20.9% to
  5.6% WER.
- **Backends:**
  - Automatic and Metal both run it; CPU, CUDA, Vulkan and ROCm are refused.
  - A GPU initialization failure is reported, never answered with a CPU model.
  - Builds without the MLX bridge do not offer the model.
- **Scope:** seven vendor-declared languages, explicit selection only; never a
  default.

Through the application it ties Parakeet v3 on held-out FLEURS en-US (5.30%
vs 5.25%) at 22 ms vs 48 ms median per clip. It also matches or beats Parakeet
on noise, quiet, German and long-form lanes. See the
[adapter record](validation/granite-speech5-turboctc-2026-10-21.md).

## Phonon-2 (Apple GPU)

Phonon-2 is a compressed Parakeet TDT v3. Vesper runs the unmodified
`mlx-community/Phonon-2` files on the pinned MLX Parakeet TDT graph. Its 264
encoder projections are replaced with a Swift packed five-value layer
(`Phonon2.swift`, architecture `phonon2`).

- **Packed layers:** each projection runs as two native MLX 2-bit affine
  matmuls with float32 scales derived from the fp16 row palettes. This matches
  the mlx-audio reference exactly.
- **Load hook:** a pinned MLX-Audio-Swift patch lets the layers be swapped in
  before strict weight verification. Stock Parakeet loading is unchanged.
- **Digit-spacing repair** (a Vesper adaptation): the model often emits digits
  without a word boundary ("about70 km", "800 ."). The adapter restores those
  boundaries and leaves acronyms, ordinals and plurals alone. This took unseen
  FLEURS WER from 9.00% to 7.43%, with no clip made worse.
- **Backends:** Automatic and Metal only, in 30 s parts; never a default.

It is the smallest Apple GPU option: ~351 MB download and 0.86 GB GPU memory.
It is less accurate than Parakeet v3 (unseen FLEURS 7.43% vs 5.68%; quiet
speech 8.88% vs 6.91%) and not faster. See the
[adapter record](validation/phonon2-2026-10-21.md).

## Parakeet Redux (Apple GPU)

Redux is offered for its size: ~217 MB and 0.94 GB of GPU memory, with v3's 25
languages and automatic detection. It is not offered for accuracy.

- **Conversion:** `scripts/convert-parakeet-redux-mlx.py`, run in the locked
  `benchmarks/asr/python/mlx-convert` project, repacks Moondream's ternary
  codes losslessly into MLX 2-bit words. It writes v3's MLX config. The output
  is hosted unmodified on the project's Hugging Face account.
- **Runtime:** `ParakeetRedux.swift` swaps the 264 encoder projections for one
  2-bit matmul each through the Parakeet load hook, and runs in float32.
- **Backends:** Automatic and Metal only, in 30 s parts; never a default.

Measured through the application against Parakeet v3:

| Lane | Redux | Parakeet v3 |
| --- | --- | --- |
| Unseen FLEURS en-US | 6.71% | 5.68% |
| 0 dB noise | 13.32% | 9.21% |
| German | 7.16% | 6.62% (CPU export) |

Long-form was 8.97% vs 10.50% (within noise). See the
[Redux record](validation/parakeet-redux-2026-10-21.md).

## Parakeet CPU precision

The built-in Parakeet TDT v3 CPU path installs 4-bit weight-only artifacts
(`parakeet-tdt-0.6b-v3-wo4/`): ONNX Runtime `MatMulNBits`, block 32, with FP32
activations. They are converted from the official sherpa-onnx FP32 export and
hosted at
[`rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4`](https://huggingface.co/rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4).

The previous dynamic INT8 export quantized activations at run time. Held-out
WER on read speech:

| Language | INT8 | 4-bit weight-only |
| --- | ---: | ---: |
| English (FLEURS) | 7.70% | 5.95% |
| German (FLEURS) | 8.23% | 6.62% |

The download size is about the same; CPU decoding is roughly 1.5× slower.
See the [precision record](validation/parakeet-weight-only-2026-10-21.md). The
scores in the next section were measured on the previous INT8 export. The
Parakeet v2 and Unified presets showed no gain from the same conversion and keep
their INT8 exports. Cohere's INT8 export matched a full-precision reconstruction
from the original weights within noise (English 5.85% vs 5.75%, German 4.74% vs
4.56%) at half the decode time, so it is unchanged.

For a source under the NVIDIA Open Model License, pass `--license other
--license-name nvidia-open-model-license --license-link URL --notice "Licensed by
NVIDIA Corporation under the NVIDIA Open Model License"`. That writes the
required `NOTICE` file next to the converted files.

Reproduce a conversion with:

```sh
uv run --locked --project benchmarks/asr/python/onnx-quantize \
  python scripts/quantize-onnx-weight-only.py \
  --repo csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3 \
  --revision 1a468a35cbba69418f126de829e75261dea4a4e4 \
  --source-model nvidia/parakeet-tdt-0.6b-v3 --out OUTPUT_DIR
```

`moondream/parakeet-ultra`, a post-trained v3, was converted exactly for both
engines and measured on 21 October 2026. It did not beat v3 (Metal 5.10% vs 5.25%;
CPU FP32 9.27% vs 8.28%), so it is not offered. See the
[parakeet-ultra record](validation/parakeet-ultra-2026-10-21.md).

`moondream/parakeet-redux`, a ternary student of v3, was first measured on
21 October 2026 and not offered. On 22 October it was added by owner decision
for its size; see [Parakeet Redux](#parakeet-redux-apple-gpu).

## Parakeet completed-audio parts

Parakeet TDT v3 and custom NeMo transducer profiles on CPU decode completed audio in contiguous parts of up to 30 s. Each part has a fresh stream, and progress is reported per part. Recordings of 30 s or less still use one native call. Before [#153](https://github.com/rahulsharmavishwakarma/vesper/issues/153), the whole recording went to a single `decode` call, which silently lost about half of the words on 60–120 s recordings.

Local measurement, 16 October 2026 (Apple M5 Pro, CPU, resident recognizer). Public FLEURS en-US clips were concatenated into three 63–118 s recordings per lane, 608 reference words each.

| Native call limit | Clean WER | 0 dB noise WER |
| --- | ---: | ---: |
| Whole recording (before) | 52.30% | 62.99% |
| 10 s | 17.43% | 29.11% |
| 20 s | 28.12% | 39.14% |
| **30 s (selected)** | **17.27%** | **36.51%** |

On the 24 individual clips (4–24 s), 30 s leaves output identical to the previous single call (15.13% clean, 25.16% at 0 dB). A 10 s limit would worsen those clips to 16.94% and 30.76%, because it cuts 10–24 s utterances mid-sentence. The 20 s result shows that a longer native call is not monotonically better for this export, so the limit is an empirical choice, not a model guarantee. The evidence covers English read speech only. Boundary words can still be misrecognized, and other languages and natural long dictation remain unmeasured. Reproduce with the ignored `installed_parakeet_long_final` test and `VESPER_LONG_FINAL_FIXTURES` set to public 16 kHz WAVs.

## Whisper extension

The shared offline engine and resident Sherpa wrapper also implement the Small/Turbo Whisper adapter under [ADR-0018](adrs/0018-shared-whisper-adapter.md). Both checkpoints use one Whisper configuration/result-policy module. They share installation, recording isolation, cancellation and segmentation with existing infrastructure; Whisper's maximum segment is 29 seconds, while Qwen/Cohere retain ten seconds.

The native patch additionally reports Whisper confidence and detected language using separate stream options, rejects oversized native input, and replaces the six-tokens-per-second heuristic with the half-context text-token allowance and fixed KV bounds. Actual EOS is required before applying the joint no-speech/log-probability policy. These Whisper changes do not alter Cohere's decoder policy or add attention/KV kernels.

The installed adapter can be exercised through `vesper-eval --model openai/whisper-small` or `openai/whisper-large-v3-turbo` with CPU completed audio. The picker offers Small and Turbo by owner decision (2026-10-07) although the noise and repetition quality gates have failed; Hindi, code-switching and singing remain unqualified. See the [dated adapter validation](validation/whisper-adapter-2026-09-18.md), which keeps source capability, actual execution, and release qualification separate.
