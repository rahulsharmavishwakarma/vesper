# ASR qualification benchmark

The user-facing language/model matrix and the distinction between compatibility and qualification are in [Language support and model recommendations](../../docs/language-support-and-recommendations.md).

The runner accepts either a built-in `--model MODEL_ID` or one validated custom speech declaration through `--asr-profile PROFILE_JSON`. These options are mutually exclusive. The profile path uses the same pinned `HuggingFaceProfile` schema, validation, artifact layout, and sherpa-onnx adapter as the application; it is not a second inference implementation. The model installer example accepts the same `--asr-profile` option so benchmark setup exercises the production download and hash-verification contract.

This benchmark runs the application's real Rust/native inference path. It is not a Python proxy and it does not treat a publisher score as a local result. Corpora, raw reports, detailed generated JSON, machine environments, and process/source inventories live below `.cache/benchmarks/` and are intentionally gitignored. Source control keeps preparation and scoring code, input manifests, capability metadata, fixtures, tests, and human-reviewed Markdown conclusions. A result is not made more trustworthy by committing a large debug snapshot.

Detailed local summaries use `.cache/benchmarks/results/<run-id>/debug-summary.json`. Before removing a legacy tracked report, preserve and hash it with `uv run --locked python scripts/archive-debug-reports.py PATH...`; `uv run --locked python scripts/archive-debug-reports.py --verify` checks the ignored local index. A publishable machine-readable result is exceptional: it must be compact, sanitized, path-independent, required by a documented consumer, and generated separately from the debug report.

[15 September 2026 continuous-speech and completed-audio screen](2026-09-15-screen.md) records local results, the supported-context quality regression, default Cohere→Qwen checks, and the limits of these measurements.

[18 September 2026 nine-model catalog screen](2026-09-18-app-catalog-en-us.md) runs every ASR choice currently selectable in the application on one comparable US-English completed-audio CPU lane. It closes execution coverage for this narrow profile, not all-language, streaming, or device qualification.

[16 September 2026 Gemma 4 single-model comparison](2026-09-16-gemma4.md) tests audio-to-polished-text before product integration, including a verified projector precision conversion, unsuccessful conversion and batch experiments, and comparisons with LFM and the default two-stage pipeline.

[Audio frontend research](../../docs/research-audio-quality.md) covers candidates up to 200 MB and separates source capabilities from [local screening results](../../docs/validation/audio-frontends-2026-09-16.md). Empty references are supported only for noise scenarios; report false transcripts and inserted words/minute separately from speech WER. The development `audio_frontend` example tests native VAD, denoising and event classification; `scripts/benchmark-desert-ant.py` separately tests Ear language ID and Uhm fillers on CPU. Paced frontend replay is not live-ASR or microphone qualification.

The [25 September Nemotron 3 diarization sample test](../../docs/validation/nemotron-diarization-2026-09-25.md)
uses `scripts/probe-nemotron-diarization.py`, a pinned isolated native build and
four local public/synthetic clips on CPU and Metal. It reports fresh-process
time, speaker diagnostics and a repeated 10 ms end-boundary failure. This is
standalone diarization feasibility. Its [application follow-up](../../docs/validation/nemotron-diarization-2026-09-25.md#application-integration-validation)
checks the separate experimental post-ASR adapter, native coexistence,
cancellation and persistence; it does not qualify ASR accuracy improvement or
live latency. Generated audio, models and JSON remain ignored.

The current frontend shortlist requires model releases from **2025-09-16 onward**; older checkpoints remain baselines. [Release evidence](../../docs/validation/audio-frontend-recency.json) distinguishes Silero v6.2's real November 2025 weight update from its newer package version, and Ear's recent export from its older Whisper checkpoint.

[Hard regression protocol](audio-regression.md) and [measured conclusions](../../docs/validation/audio-regression-2026-09-16.md) extend this screen with 36 unused natural recordings, ten signal conditions, annotated AMI filler/backchannel excerpts, paired Cohere→Qwen comparisons, tiny/variable packet boundaries and three-minute frontend replay.

The [17 September application verification results](../../docs/validation/application-sha-2026-09-17.md) use `scripts/qualify-application-sha.py` to compare frozen software/hardware SHA builds through the actual Cohere→Qwen adapters, using that same public corpus. It runs two order-reversed blocks of 15 completed recordings per build, then five diagnostic stage cases per build. It preserves process/row failures, compares exact transcripts and completion results, and measures ASR/writer stages and macOS process RSS. It does not measure capture or UI latency. Freeze both `audio_frontend` and `cohere_stage_probe` release examples under the output's `binaries/` directory with `software-` and `hardware-` prefixes before running; no compilation occurs during timed inference. Existing results cannot be overwritten.

The [shared request-path regression](../../docs/validation/supported-request-optimization-2026-09-17.md)
uses `scripts/check-supported-inference.py` with frozen `baseline-eval` and
`candidate-eval` release binaries. It compares eight completed recordings per
ASR through the actual two-stage pipeline, then Nemotron accelerated and paced
180-second streaming/reset cases. `--models NAME --offline-only` supports a
separate focused follow-up. Preserve failed attempts when restoring missing
artifacts; never replace their reports with successful reruns. The dated result
records all model/source hashes and the limits of its CPU English evidence.

The [Cohere residency qualification](../../docs/validation/cohere-residency-2026-09-17.md)
compares frozen `baseline-eval`/`candidate-eval` binaries with
`scripts/qualify-cohere-residency.py OUTPUT CORPUS FRENCH_CORPUS`. Its two
English blocks reverse build order and preserve the same 30 recordings/build;
separate diagnostic and French lanes retain all failures. The production manager
reports a resident load ordinal and per-request ordinal, so repeated load timing
fields must only be counted once per lifetime. Full source/artifact hashes,
100-cycle memory/invalidation checks and metric definitions are in the report.

The [18 September Whisper native screen](../../docs/validation/whisper-2026-09-18.md) uses the development `whisper_probe`, pinned [ONNX artifacts](whisper-artifacts.json), and `scripts/qualify-whisper.py`. It preserves completed-audio, writing, language/noise and duration-guard outcomes. Observed contention prevents an isolated latency claim, and the probe is not production Whisper support. Preparation, source fingerprints, failed attempts and remaining integration gates are in the report.

The [18 September VibeVoice CPU reference screen](../../docs/validation/vibevoice-2026-09-18.md) uses the pinned Microsoft step API, `scripts/probe-vibevoice.py`, and the canonical Rust `score_json` helper. The [family inventory](vibevoice-inventory.json) separates total parameters from backbone labels and quantized file sizes. No complete official VibeVoice ASR model satisfies a strict <2B total limit; the requested Streaming-1.5B is an out-of-budget reference, with no production adapter.

The maintained Python probe now uses Transformers 5.10.1 with a
[verified publisher-source patch](../../patches/vibevoice/README.md). Follow the
[current compatibility checks](../../docs/development.md#python-tooling-policy)
before replaying those plans. The source/checkpoint revisions remain pinned;
the probe verifies all loaded tensors, tying and streaming cache progression.
Historical reports describe their original environments and do not transfer
unchanged to this port.

The subsequent [quantized BitNet CPU benchmark](../../docs/validation/vibevoice-bitnet-2026-09-18.md) executes the verified specialized GGUF pair using the official native CLI and resident-file server. It includes matching corpus controls, thread and normalization ablations, a fresh Cohere comparison, raw-output/completion instrumentation and a separately measured decoder-position correction. Memory-blocked long input and failed hotword/JSON output remain visible. The native server's token output is not an incremental-audio streaming adapter; this development benchmark does not make the capability entry executable in the application.

### Python environments

Use uv and the versioned `pyproject.toml` / `uv.lock` for each environment:

| Project | Purpose | Python/platform |
| --- | --- | --- |
| Repository root, `dev` group | Tests, NumPy/soundfile preparation and summaries | Python 3.12+ |
| Repository root, `benchmark` group | PyArrow corpus preparation | Python 3.12+ |
| `benchmarks/asr/python/audio-frontend` | ONNX Runtime and LiteRT frontend probes | Python 3.12, macOS Apple silicon |
| `benchmarks/asr/python/vibevoice` | Frozen Torch/Transformers reference probe | Python 3.12, macOS Apple silicon |

Run scripts from the repository root with `uv run --locked python scripts/SCRIPT.py`,
adding `--group benchmark` for PyArrow or `--project` for an isolated probe.
`--project` selects the environment while retaining the caller's working directory.
Each project is non-distributable and keeps its `.venv` ignored. Direct versions
are pinned; resolving the environment is not model qualification. Run
`make audit-python-audio-frontend` and `make audit-python-vibevoice` on
Apple-silicon macOS before new benchmark work. See the
[known dependency advisories and upstream blockers](../../docs/ci-and-releases.md#dependency-security-status-and-upstream-limits).

The two requirements files are generated compatibility exports, not editable
dependency sources. Update the owning manifest and lockfile first, regenerate
the export as described in [CI and releases](../../docs/ci-and-releases.md), and
run `make python-lock-check`. Dated reports preserve their original setup commands
as historical evidence; use this section for new runs. The normal contributor
install does not install Torch, ONNX Runtime or LiteRT.

### Reproduce the frontend screen

The completed [19 September denoising follow-up](../../docs/validation/denoising-followup-2026-09-19.md) records 2,153 attempts using fixed original/enhanced mixtures, known-gain diagnostics and bounded-observer streaming. Mixing helps several speech cases but introduces noise regressions; input level is a substantial contributor to quiet-speech errors. Native contracts and corpus reproduction pass, while contention prevents an isolated latency claim. Run instructions and evidence limits are in the report; detailed JSON stays in the ignored result cache and application defaults remain unchanged. Its matched-level extension adds 720 main and 24 confirmation attempts, supporting input-level conditioning while retaining the failed noise/default gates. Derived manifests and detailed results remain local; the tracked source selection is reused.

Prepare the public FLEURS corpora using the instructions below. The frontend asset helper copies matching files from those directories, verifies every audio hash, and can generate the fixed non-speech and macOS Samantha controls. Synthesized audio depends on the voice/OS build: a mismatched hash is a new corpus, not permission to silently replace the pinned reference. Models stay in the ignored benchmark cache; these are development dependencies only.

```sh
uv sync --locked --project benchmarks/asr/python/audio-frontend

uv run --locked python scripts/prepare-audio-frontend-assets.py \
  --manifest benchmarks/asr/manifests/audio-frontends-screen.json \
  --corpus .cache/benchmarks/audio-frontends/corpus \
  --source-corpus .cache/benchmarks/datasets/fleurs-en-us \
  --source-corpus .cache/benchmarks/datasets/fleurs-hi-in \
  --generate-controls --download-models

source scripts/dev-env.sh
cargo build --release -p vesper-eval --example audio_frontend --locked
```

Fill in an environment JSON using the existing template. Run from the repository root: the native example's frontend artifact directory is `.cache/benchmarks/models/audio-frontends`. Its ten required arguments are artifact manifest, frontend ID, corpus manifest, corpus directory, existing ASR model directory, environment JSON, output JSON, ASR mode, delivery mode and signal condition.

```sh
target/release/examples/audio_frontend \
  benchmarks/asr/audio-frontend-artifacts.json silero-v6.2 \
  benchmarks/asr/manifests/audio-frontends-screen.json \
  .cache/benchmarks/audio-frontends/corpus models/downloads \
  .cache/benchmarks/audio-frontends/environment.json \
  .cache/benchmarks/audio-frontends/silero-v6.2.json none offline clean
```

Frontend IDs: `raw`, `silero-v6.2`, `dpdfnet4`, `dpdfnet8`; historical baselines `silero-fp32`, `silero-int8`, `gtcrn`, `ced-mini`, `ced-base`. ASR modes: `none`, `cohere`, `lfm`. Delivery: `offline`, `chunks`, `realtime`; CED remains completed-clip classification. Conditions: `clean`, `quiet`, `noise0`, `noise10`. VAD records decisions while passing original audio through. ASR uses explicit English/Hindi test prompts and retains unsupported-language failures; LFM tests require English. Reports save content-addressed input/output PCM for alignment inspection. An optional eleventh argument specifies comma-separated packet sizes (default320). Frontend-only runs permit up to600seconds; ASR remains bounded at60seconds. Do not parallelize timed model runs.

The continuous manifest describes the first three English screening clips concatenated twice, with no added silence, followed by a fresh first clip. Its 54.92-second recording is an artificial continuous-stream diagnostic. The filler manifest includes four additional Samantha140wpm controls. Exact recipes, hashes and exclusions are in the manifests and dated report; none establishes natural dictation qualification.

For Ear, obtain the author fixture `Tests/EarTests/Resources/ear_frontend_golden.json` from SDK revision `2aed9c6d6459fc1843ab59553965035954e40199`; expected SHA-256 is `2ec5d84d57fc7c526360cd4c1a6745ccc4e669b4db613241f8907976d1907b7e`. It is checked numerically before inference. Use CPU thread caps and the pinned environment:

```sh
OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 \
  uv run --locked --project benchmarks/asr/python/audio-frontend python scripts/benchmark-desert-ant.py ear \
  --artifacts benchmarks/asr/audio-frontend-artifacts.json \
  --model-root .cache/benchmarks/models/audio-frontends \
  --manifest benchmarks/asr/manifests/audio-frontends-screen.json \
  --corpus .cache/benchmarks/audio-frontends/corpus \
  --environment .cache/benchmarks/audio-frontends/environment.json \
  --golden .cache/benchmarks/audio-frontends/ear_frontend_golden.json \
  --output .cache/benchmarks/audio-frontends/ear.json
```

Uhm uses `uhm-raw` and `uhm-normalized` as separate modes with the filler manifest; it does not require `--golden`. The probe is single-window (up to30s), validates the actual six-class graph and annotates spans without deleting audio. `scripts/summarize-audio-frontends.py` regenerates the dated suite's transcript-free aggregate from its versioned raw report filenames, keeping failed/missing cases and separate language/corpus lanes.

[Local correction-memory corpus protocol](personalization.md) measures frozen application memory on separate calibration and holdout recordings, with a smaller real-time replay subset. It is distinct from base-model qualification.

[16 September 2026 local memory results](2026-09-16-personalization.md) report real public audio, paired correction/cleanup measurements, normal-speed replay, regressions and remaining personal-learning coverage gaps.

[16 September 2026 automatic learning](2026-09-16-automatic-learning.md) tests ordinary-edit admission, usage/retry evidence and chronological public-hypothesis replay. It reports component latency separately from ASR and keeps the public replay's lack of automatic accuracy gain visible.

[17 September 2026 bounded policy trial](2026-09-17-policy-learning.md) tests a separate offline proposal/validation/rollback loop. It compares all fixed policies, preserves adverse final cases and documents why the results do not justify live policy adaptation.

The [18 September stress evaluation](2026-09-18-policy-learning-stress.md) expands this to 97 seeded histories and 23,809 episodes, with correlated-feedback regression fixes, capacity/protected-text checks and hidden-label isolation. It remains a text/feedback simulation; its component timings are not ASR or stop-to-final latency.

The [18 September real-audio accent evaluation](2026-09-18-accent-personalization.md) instead runs 304 new Svarah recordings across 19 primary-language groups through local Nemotron/Cohere, with 38 paced streaming recordings and separate FLEURS controls. It reports per-group accuracy, dictionary regressions, actual native thread counts and failures. Language background is an accent proxy, not an acoustic accent classification or proof of learned adaptation.

[Expanded memory regression testing](2026-09-16-memory-regressions.md) records quote/negation/suggestion/parser fixes, 778 controlled cases, 48 fresh recordings, real storage workflows and native reset/cleanup checks.

## What is measured

- exact, normalized word error rate and Unicode grapheme error rate;
- audio decode/setup time, ASR time, cleanup time, and real-time factor;
- first nonempty partial, stop-to-final latency, partial count, and edit distance between partials;
- cold versus genuinely resident Nemotron runs;
- model/runtime revisions, hashes, backend, precision, hardware envelope, corpus license, and sample hashes;
- failure count. Failed samples never disappear from an aggregate.

`--realtime` replays 20 ms packets against cumulative wall-clock deadlines. Without it, streaming is an accelerated throughput/soak test. A completed-file adapter is not labelled streaming. The current memory field remains `null` until peak RSS is captured by the platform runner; do not invent it from model file size.

Exact WER deliberately scores Latin words against Devanagari transliterations as different. This is appropriate for exact transcript evaluation, but reports should also be read qualitatively for mixed-script text.

### Unused cached FLEURS qualification samples

`scripts/prepare-qualification-holdout.mjs REPOSITORY NEW_OUTPUT` freezes 32
English-US and 32 Hindi-India read-speech samples before inference. It verifies
the cached archive/metadata hashes against the existing source manifests, scans
versioned and cached benchmark JSON/JSONL for prior audio IDs and references,
excludes matching sentence groups, then selects unique 4–20-second sentences in
a fixed hash order. Exclusions, scanned-file hashes, attribution and per-audio
hashes remain in the new ignored output directory. Pass its `exclusions.json`
as the third argument to reproduce the selection after more experiments exist.
Existing outputs cannot be overwritten. This establishes exclusion from the
scanned experiment history, not from model training or unavailable history;
it does not establish independent speakers, microphone behavior or singing.

Run the ordinary evaluator separately for each supported model/backend/language
combination. Keep explicit-language and automatic-language lanes separate.
Do not tune decoder/frontend thresholds on these results. Freeze source,
executable and native artifact identities before launch and compare afterward;
record failed processes and failed sample rows rather than dropping them.

## Phase-one language packs

| Pack | Benchmark language | Primary held-out source | Purpose |
| --- | --- | --- | --- |
| Hindi | `hi-IN` | FLEURS `hi_in` test | Monolingual Devanagari read speech |
| Hinglish | `hi+en` | MUCS Hindi-English test | Intra-utterance code switching and mixed script |
| Indian English | `en-IN` | AI4Bharat Svarah test | Indian accents, read and spontaneous speech |
| General US English | `en-US` | FLEURS `en_us` test | International read-speech baseline |

The locale or pair is part of the evidence key. `en-IN` and `en-US` must never be pooled, and `hi+en` is not a Hindi result. The application prompt remains `hi`, `en`, or `auto`; by default the runner maps locale variants to their base prompt and a language pair to `auto`.

Download the two public FLEURS packs read-only with the official Hugging Face CLI at the pinned revision:

```sh
hf download google/fleurs \
  data/hi_in/test.tsv data/hi_in/audio/test.tar.gz \
  data/en_us/test.tsv data/en_us/audio/test.tar.gz \
  --repo-type dataset \
  --revision 70bb2e84b976b7e960aa89f1c648e09c59f894dd \
  --local-dir .cache/benchmarks/downloads/fleurs
```

After inspecting the archive paths, extract each test archive and prepare deterministic unique-sentence subsets:

```sh
node scripts/prepare-fleurs-benchmark.mjs hi_in \
  .cache/benchmarks/corpora/fleurs-hi-in/test \
  .cache/benchmarks/downloads/fleurs/data/hi_in/test.tsv \
  .cache/benchmarks/datasets/fleurs-hi-in --count 16

node scripts/prepare-fleurs-benchmark.mjs en_us \
  .cache/benchmarks/corpora/fleurs-en-us/test \
  .cache/benchmarks/downloads/fleurs/data/en_us/test.tsv \
  .cache/benchmarks/datasets/fleurs-en-us --count 16
```

Svarah is gated but CC-BY-4.0. Accept its access terms, use a read-only token, and pin revision `ebbf7777fe771490696a3f7b007097606fa8c924`:

```sh
hf download ai4bharat/Svarah \
  data/test-00000-of-00003.parquet \
  data/test-00001-of-00003.parquet \
  data/test-00002-of-00003.parquet \
  --repo-type dataset \
  --revision ebbf7777fe771490696a3f7b007097606fa8c924 \
  --local-dir .cache/benchmarks/downloads/svarah
```

Its Parquet extraction is intentionally separate from the dependency-free FLEURS/MUCS preparation path.

The pinned source checksums used by the preparer are:

| Source object | SHA-256 |
| --- | --- |
| FLEURS `hi_in` test audio archive | `47c238b9cf7016ba596639bb3038b0c89dc6e85759f16d09c4f58c6dfa25c65f` |
| FLEURS `hi_in` test metadata | `889e82e2875490f9533ff59ab67326fcfa01cf828fc4412758aaaa1f442a729f` |
| FLEURS `en_us` test audio archive | `d9c2e37b41aacd41bc283554a0a82b5476b36887049774ecb2819dcaaa55a356` |
| FLEURS `en_us` test metadata | `74c046239374deeb60fa63f258f907388093a32bcaa3140965f70ef05c79f7ca` |
| Svarah test shard 0 | `ac8fc08edabe47dd0ef915276bc0fab51ec1037c65794c7be8f98c5bc746d1db` |
| Svarah test shard 1 | `2adf4c4ce1e69e457f44d3af0756b3965bd05fca409b4cd27528ef5804502fba` |
| Svarah test shard 2 | `1cb3f5c6aa0c18e8c6d106f0be6129a35ddaeeb9fc6977ecb059b8f36033fd23` |

## Reproduce the Hinglish suite

Download the public MUCS test archive, verify it, inspect paths, and extract it:

```sh
mkdir -p .cache/benchmarks/downloads .cache/benchmarks/corpora/mucs-hi-en-test
curl --fail --location --retry 5 --retry-all-errors \
  --output .cache/benchmarks/downloads/Hindi-English_test.tar.gz \
  https://www.openslr.org/resources/104/Hindi-English_test.tar.gz
echo '93e358b3bf8233a897fcd353c1f4f98fdda6b8c01b7eed17a70c7dd26e984b37  .cache/benchmarks/downloads/Hindi-English_test.tar.gz' | shasum -a 256 -c -
tar -tzf .cache/benchmarks/downloads/Hindi-English_test.tar.gz
tar -xzf .cache/benchmarks/downloads/Hindi-English_test.tar.gz \
  -C .cache/benchmarks/corpora/mucs-hi-en-test
```

The archive is the MUCS 2021 Hindi-English test set from OpenSLR 104, licensed CC-BY-SA-4.0. Its benchmark language is `hi+en`, not monolingual Hindi. Keep the attribution emitted in the generated manifest with any redistribution.

Prepare 16 deterministic, distinct-speaker clips plus a natural long recording without re-encoding PCM:

```sh
node scripts/prepare-mucs-benchmark.mjs \
  .cache/benchmarks/corpora/mucs-hi-en-test/test \
  .cache/benchmarks/datasets/mucs-hi-en \
  --count 16 --include-long
```

Copy `environment.example.json` outside the source tree and replace every placeholder. A CPU environment must use a CPU runtime package; an `Auto` run that falls back from Metal is not a CPU attestation.

Run optimized inference, changing paths and model IDs as needed:

```sh
. ./scripts/dev-env.sh
cargo run --release --locked -p vesper-eval -- validate \
  .cache/benchmarks/datasets/mucs-hi-en/manifest.json

cargo run --release --locked -p vesper-eval -- run \
  .cache/benchmarks/datasets/mucs-hi-en/manifest.json \
  .cache/benchmarks/datasets/mucs-hi-en \
  models/downloads \
  .cache/benchmarks/environment.json \
  .cache/benchmarks/results/nemotron-hi-streaming.json \
  --model nvidia/nemotron-3.5-asr-streaming-0.6b \
  --language hi --streaming --realtime --chunk-ms 20
```

Use accelerated replay for recordings that would make wall-clock CI impractical. Keep at least one real-time subset because throughput does not establish interactive latency.

## Compare and recommend

Only combine reports with the same corpus selection, reference language, configured language (`auto` or an explicit language), mode, backend, precision, and hardware envelope. The summarizer rejects mismatched manifests, machine/runtime envelopes, and replay pacing; it keeps Auto and explicit-language evidence separate and preserves failures:

```sh
node scripts/summarize-asr-benchmarks.mjs \
  --out .cache/benchmarks/results/evidence.json \
  .cache/benchmarks/results/*.json

cargo run --locked -p vesper-eval -- recommend \
  benchmarks/asr/request.hi-offline-cpu.example.json \
  benchmarks/asr/capabilities.json \
  .cache/benchmarks/results/evidence.json
```

Use `request.hi-auto-offline-cpu.example.json` when the user selected automatic language detection. Explicit-Hindi measurements cannot satisfy that request.

The recommendation gate first excludes incompatible language, mode, backend, duration, auto-language, code-switching, and singing requirements. Comparable local evidence ranks ahead of official-only capability. Requests explicitly choose `accuracy`, `latency`, or `balanced`; balanced scoring considers both median WER and median real-time factor instead of hiding a large latency regression behind a small accuracy gain. Optional maximum WER, RTF, stop-to-final, and failure-rate fields turn the request into a qualification policy. A comparable measured candidate that fails one of those ceilings is excluded rather than falling back to an “official” label. An official-only candidate remains visibly unmeasured.

The checked-in examples use a 20% median-WER ceiling for monolingual packs, 30% for `hi+en`, 1.0 median RTF, and 2% failure rate. These are initial product gates, not scientific constants; change them only with a documented validation set and review. Current development-machine evidence qualifies Nemotron for Hindi and US English under the balanced objective. Neither measured adapter qualifies on the Hinglish suite, so that request correctly returns an empty recommendation list.

A separate 48-recording Svarah `en-IN` CPU screen measured raw WER of 18.50% for Nemotron and 9.83% for Cohere, with 48/48 successful executions for each. That run is part of the frozen personalization experiment documented in [the local correction-memory report](2026-09-16-personalization.md). Reports carrying a personalization profile are deliberately rejected by the base-model recommendation summarizer, so this narrow result is local Indian-English evidence but does not automatically qualify or promote either base model. A clean all-model `en-IN` recommendation matrix remains outstanding.

A candidate with `"executable": false` is never a recommendation; it may have a working development adapter whose release qualification is incomplete. Adding a newer model means adding its architecture adapter, pinning its artifacts, testing the same held-out manifests, and only then changing that flag or a default.

Do not tune on these held-out clips. Training and threshold selection need separate data; this suite is for evaluation and regression detection.

## Shared Whisper adapter qualification

Use `scripts/qualify-whisper.py ... --application` to run Small, Turbo and Cohere through the same frozen `vesper-eval` binary. The default without `--application` remains the historical native-probe protocol. Build once before timing; record source, binary, native-library and artifact hashes, and keep competing work out of the measurement window. `--english-only` creates a separate repeated English pass; preserve the initial pass rather than overwriting it.

```sh
uv run --locked python scripts/qualify-whisper.py RESULT_DIR WHISPER_ARTIFACT_DIR AUDIO_REGRESSION_DIR FLEURS_LANGUAGE_DIR --application
uv run --locked python scripts/summarize-whisper.py RESULT_DIR PUBLICATION.json --repeat-root ENGLISH_REPEAT_DIR
```

`RESULT_DIR` must contain `binaries/vesper-eval`, `environment.json`, `native-provenance.json`, `source.json` and `post-freeze-changes.json`; the runner creates manifests, process logs and a failure ledger. The historical artifact-directory argument is unused in application mode because the real installer owns artifact selection. The summarizer separates cleanup failures from ASR failures and excludes bogus non-speech scores for the historical unreferenced stitched-speech soak. Keep its original report hash and execution evidence.

Run the installed-model lifecycle tests sequentially with the same pinned fixtures:

```sh
export VESPER_SMOKE_MODELS=models/downloads
export VESPER_SMOKE_AUDIO=PUBLIC_ENGLISH_A.wav
export VESPER_COHERE_AUDIO_B=PUBLIC_ENGLISH_B.wav
export VESPER_COHERE_LONG_AUDIO=PUBLIC_MEETING_OVER_29_SECONDS.wav
export VESPER_COHERE_FRENCH_AUDIO=PUBLIC_FRENCH.wav
cargo test --release --locked -p vesper-inference installed_whisper_small_residency_isolation_and_invalidation -- --ignored --nocapture --test-threads=1
cargo test --release --locked -p vesper-inference installed_whisper_turbo_residency_isolation_and_invalidation -- --ignored --nocapture --test-threads=1
cargo test --release --locked -p vesper-inference installed_cohere_residency_isolation_and_invalidation -- --ignored --nocapture --test-threads=1
```

The shared test performs 100 mixed cycles, cold-versus-resident text comparisons, fresh-state resets, model/language changes, cancellation and copied-artifact invalidation. The legacy Cohere fixture variable names are reused for all three models. For timing, prebuild and run the frozen test executable instead of compiling alongside another benchmark. The [dated adapter report](../../docs/validation/whisper-adapter-2026-09-18.md) defines the observed envelope, quality failures and remaining release gates.

### Shared audio frontend qualification

The opt-in automatic-level policy uses `vesper-eval run ... --auto-level`,
independently of `--vad` annotations and `--denoiser`. Its application-path
qualification helper is `uv run --locked python scripts/qualify-automatic-level.py`:
run `prepare` once, then `development`, `quality`, `streaming`, `soak`, and
`summarize` after building the release evaluator. Cached public FLEURS/MS-SNSD
audio from the existing frontend benchmarks is required. `prepare` refuses to
overwrite prior input; binaries, source snapshots, generated manifests and
detailed results stay in the ignored benchmark cache. Preserve earlier attempts
when a development revision changes. The fixed matrix separates fresh read
speech, artificial attenuation/noise, reused noise/meaning controls and paced
continuous replay. It does not qualify real whispers, physical microphones,
other languages or target devices. See [the frontend ADR](../../docs/adrs/0019-shared-audio-frontend.md)
and [measured automatic-level results](../../docs/validation/denoising-followup-2026-09-19.md#automatic-level-implementation-and-qualification).
The 468-attempt recorded-audio matrix has mixed accuracy and preserved ASR/noise
failures; successful execution does not justify enabling this policy by default.

The [application-path report](../../docs/validation/audio-frontend-integration-2026-09-18.md) separates completed-audio quality, paced replay, native contracts, preserved failures and remaining qualification gaps.

The application evaluator accepts `--vad` and `--denoiser off|dpdfnet4|dpdfnet8`; these invoke the same manager frontend as the desktop, including warnings/fallback evidence. They work with completed recordings and `--streaming --realtime --chunk-ms 20` for every current ASR adapter. Native streams and buffered previews are distinguished in execution evidence; buffered paths and Qwen MLX previews run an authoritative full-recording pass at Stop.

Use `scripts/prepare-frontend-integration.py` with the root uv environment and cached public FLEURS/MS-SNSD/regression assets to freeze a new selection before inference. Build `cargo build --release -p vesper-eval`, then run `uv run --locked python scripts/qualify-frontend-integration.py streaming`, followed by the `quality` and `soak` phases sequentially. The runner freezes the binary/source hashes, preserves failed processes, records competing work and sampled RSS, and never changes settings based on results. The soak is 30 minutes of paced synthetic repetition and is excluded from accuracy. Frozen attributed public references and recipes are in [the dated manifests](manifests/frontend-integration-2026-09-18/quality.json). Once those manifests exist, preparation rebuilds that exact selection and checks every generated audio hash; it refuses an existing output directory. Original waveforms remain local. The initial run is under `.cache/benchmarks/results/frontend-integration-2026-09-18`; it includes a retained metadata-only split-schema preflight failure. Do not overwrite completed evidence to restart a run.

Installed contracts can be run using `VESPER_SMOKE_MODELS` and `VESPER_SMOKE_AUDIO` with the ignored tests `installed_frontends_packetization_tails_and_100_recording_resets`, `live_frontend_cancel_fallback_and_reset_isolation`, `live_frontend_load_failure_uses_original_audio_and_retains_asr_errors`, and `live_frontend_device_rates_match_completed_processing`. The live tests need the pinned CPU Nemotron runtime/model plus the same frontend artifact layout. They use public speech fixtures. Full hashes are checked before native model construction. Device-rate tests compare irregular 44.1/48 kHz delivery against completed processing of the same synthetic-rate waveform; they do not substitute for microphone quality qualification.

## Common live-adapter qualification

Use the same manifest, language, backend, pinned artifacts and frontend options
for paired completed and `--streaming --realtime --chunk-ms 20` runs. Check
nonempty output **before** input ends, not just process success. A successful
completed fallback with zero drafts fails the live-output gate. Keep per-model
errors, empty drafts, cancelled runs and timeouts in the aggregate.

`firstPartialMs` starts before loading. Subtract the separately reported
`execution.liveStartup.milliseconds` to estimate the first draft after readiness.
`stopToFinalMs` includes full-audio final decoding for buffered and native Qwen
preview paths. Report cold startup separately from resident inference. Draft
events and edit distance do not establish correct-word emission time or physical
microphone-to-pixel latency. Adapters using a completed-audio final must match their paired completed path.
Native Nemotron can differ between streaming and offline decoding: retain those
differences and compare WER/CER and meaning, without claiming exact parity.

See [live adapters](../../docs/live-transcription.md) for the bounded scheduling,
resource heuristics, installed-model lifecycle test and measured qualification.
Raw audio, model files and detailed JSON remain local ignored evidence.
