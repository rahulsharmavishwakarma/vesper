# Qwen3-ASR and Cohere

The application has dedicated CPU adapters for Qwen3-ASR 0.6B and Cohere Transcribe. Select them in **Models**, choose a compatible transcription language in **Dictate** or **Settings**, then use **Save & download**. The language menu lists only modes supported by the active adapter; it guides recognition and does not translate the transcript. Selecting a model alone does not download it. They process completed recordings and imported files without an application duration cutoff. Long audio is processed internally in bounded parts; see [ADR-0011](adrs/0011-segment-long-generative-asr.md). The old recording-duration preference is ignored.

| Model | App language choices | Pinned download |
| --- | --- | ---: |
| Qwen3-ASR 0.6B INT8 | Automatic, English, French, Hindi | 987,015,347 bytes |
| Cohere Transcribe INT8 | Explicit English or French | 2,888,052,036 bytes |

Cohere has no automatic language detection or Hindi support in this adapter. Broader language-picker expansion is separate work. Both adapters accept Automatic/CPU compute settings. Qwen3-ASR is a speech model, separate from the Qwen cleanup model. Their files cannot be imported as a four-file NeMo transducer profile.

## Completion contract

Sherpa 1.13.8 exposes both native model configurations, but its ordinary result omits the reason decoding stopped. The application maintains a small reporting patch against upstream commit `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`; approval and scope are recorded in [issue #14](https://github.com/rahulsharmavishwakarma/transcribe/issues/14).

The native patch publishes `vesper.decode_report.v1` using the existing per-stream option API. It reports `schemaVersion`, `stopReason`, and `inputContextTruncated`. The app accepts ordinary text only after actual EOS with no discarded input context. Output/context caps, repetition termination, numerical/token errors, unknown versions, and missing reports reject the result. A native silence/no-input result is accepted only with empty text. Every decode clears the previous report/result. Qwen-ASR owns one recognizer per job. The application worker retains Cohere sessions under [ADR-0017](adrs/0017-resident-cohere-recognizer.md), verifying artifacts on load and invalidating on observed file/options changes. Every segment still gets a fresh stream; cancellation is checked before/after native calls, which cannot be interrupted mid-decode.

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

## Whisper extension

The shared offline engine and resident Sherpa wrapper also implement the Small/Turbo Whisper evaluation adapter under [ADR-0018](adrs/0018-shared-whisper-adapter.md). Both checkpoints use one Whisper configuration/result-policy module. They share installation, recording isolation, cancellation and segmentation with existing infrastructure; Whisper's maximum segment is 29 seconds, while Qwen/Cohere retain ten seconds.

The native patch additionally reports Whisper confidence and detected language using separate stream options, rejects oversized native input, and replaces the six-tokens-per-second heuristic with the half-context text-token allowance and fixed KV bounds. Actual EOS is required before applying the joint no-speech/log-probability policy. These Whisper changes do not alter Cohere's decoder policy or add attention/KV kernels.

The installed adapter can be exercised through `vesper-eval --model openai/whisper-small` or `openai/whisper-large-v3-turbo` with CPU completed audio. Product selection stays disabled: noise and repetition quality gates have failed, and Hindi/code-switching/singing/live streaming are unqualified. See the [dated adapter validation](validation/whisper-adapter-2026-09-18.md), which keeps source capability, actual execution, and release qualification separate.
