# Nemotron 3 Diarization: local sample test, 25 September 2026

Issue: [#42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42).
The first sections record the isolated upstream CLI smoke test, **not application
integration or ASR qualification**. The later [application validation](#application-integration-validation)
records the subsequently approved integration separately. Four fixed samples
were run on CPU and Metal; the first
meeting was repeated on both with verbose provider diagnostics. All ten native
processes completed successfully. Four outputs (the original and repeated
45-second clip on both backends) fail the strict audio-boundary check: the last
segment ends at 45.010 seconds for exactly 45.000 seconds of audio.

## Measurements

Every row starts a fresh process. Wall time includes model loading, input
reading, diarization, RTTM writing and process shutdown, measured with a monotonic
clock around `/usr/bin/time -l`. It excludes downloads/build, sample preparation
and the probe's preflight hashes. Filesystem/shader caches were not flushed.
These are completed-file times, **not microphone, first-speaker, endpoint or
stop-to-final latency**. CPU and Metal were sequential, with no concurrent build.

| Sample                      | Audio duration | Reference / detected speakers, both backends | CPU wall | Metal wall | CPU / Metal peak RSS |
| --------------------------- | -------------: | -------------------------------------------- | -------: | ---------: | -------------------: |
| AMI ES2004a, 600–645 s      |        45.00 s | 4 / 4; end-boundary failure                  | 10.456 s |    1.057 s |    238.0 / 193.2 MiB |
| AMI ES2004a, 675–705 s      |        30.00 s | 4 / 4                                        |  2.581 s |    0.594 s |    200.2 / 179.1 MiB |
| FLEURS en-US single speaker |         4.62 s | 1 / 1                                        |  0.561 s |    0.566 s |    163.5 / 159.4 MiB |
| Generated digital silence   |        10.00 s | 0 / 0                                        |  0.569 s |    0.567 s |    173.0 / 163.5 MiB |

The four-sample arithmetic mean is 3.542 s CPU and 0.696 s Metal; the mixed
durations and silence make that a smoke-matrix average, not expected dictation
latency. Keeping only the three speech clips gives 4.533 s and 0.739 s.
The first CPU result is retained: its same-input confirmation took 4.083 s,
versus 1.059 s Metal. Startup/cache/system effects were not isolated, so no
stable CPU speedup or p95 claim follows. Confirmation RSS was 225.8 / 192.7 MiB.
Process RSS does not measure all GPU/unified-memory allocations or resident app
memory; sustained energy and thermal behavior were not measured.

### Speaker accuracy diagnostics and failure

The two windows contain 9.352 s and 4.195 s of annotated overlapping activity.
The probe integrates intervals exactly, globally matches anonymous speaker IDs
one-to-one, includes overlap, and uses zero boundary collar. Its seven analytical
checks cover permutation invariance, missing overlap, complete miss, silence
false alarms, duplicate reference intervals, extra speakers and invalid numbers.

For the 30-second window, **segment-reference DER is 18.44% CPU / 18.47% Metal**:
37.031 reference speaker-seconds, 6.507 missed speaker-seconds, 0.042 confusion
seconds, and 0.281 / 0.291 false-alarm seconds. These are coarse AMI transcript
segment spans, which can include pauses/vocalizations. They are **not the official
AMI scoring recipe or directly comparable to NVIDIA's published DER**. Same
speaker count does not imply correct activity boundaries or complete overlap.

The raw 45-second output remains failed in every probe summary. An explicitly
separate diagnostic, clipping the final end to the known audio boundary before
calling `score`, yields 19.10% on both backends. This diagnostic neither repairs
the saved output nor qualifies its contract. The 10 ms overrun reproduced on
both backends; any future adapter must validate/clip frame padding against exact
audio duration and retain malformed-output reporting. No runtime patch or app
workaround was installed in this test.

Silence produced no segments; FLEURS produced one. FLEURS lacks activity-boundary
references here, so it has no reported DER. No text was decoded: WER, spelling,
accent adaptation and overlap-word recovery remain untested. Four clips from
one meeting, one English reader and synthetic silence cannot qualify language,
speaker identity, noisy microphones, singing or long-session behavior.

## Reproducibility

- Host: Apple M5 Pro, 18 logical CPUs, 64 GiB unified RAM; macOS 27.0 build
  26A428; AppleClang 21.0.0.21000334; CMake 4.4.3; ffmpeg 9.0.2.
  `pmset` reported AC Power, power mode 0, with the battery also reporting
  discharging; preserve that mixed snapshot. No thermal/performance warning
  had been recorded. No sustained power/temperature instrumentation was used.
- Application research base `0a2be4a`; shared app checkout inspected at
  `59c6d21`. No app binary participates. The source build still prints `0.1.0`
  in `doctor`; that string alone does not identify this post-release build.
- [NeMo-Speech.cpp](https://github.com/NVIDIA/NeMo-Speech.cpp/tree/97a15afa5caa9bce5baaa86c1184103877af4101)
  `97a15afa5caa9bce5baaa86c1184103877af4101`; ggml
  `c03b4e2bcece5134827881af90242086daf75be5`, with the pinned repository's patch
  series applied by `scripts/configure.sh`. No custom inference patches.
- Official [GA model](https://huggingface.co/nvidia/Nemotron-3-Diarization/tree/f667ed73aee57d40cc39428eb768b4fd87a0a29e)
  revision `f667ed73aee57d40cc39428eb768b4fd87a0a29e`, Q8_0 GGUF, 107,012,128 bytes;
  downloaded SHA-256 `08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1`.
  Q8_0 describes the artifact, not uniform arithmetic precision in every operator.
- CLI SHA-256 `d83ce39488956d4562e3e5280767387f0022088fbd796990a904f5d1ceab3202`.
  Dynamic-library hashes, patched ggml diff hash and build logs are retained
  under the ignored cache's `build-fingerprints.json`.
- Native `metal-diar` Release preset, Unix Makefiles generator, microphone
  capture disabled. SentencePiece 0.1.99 was built statically from the existing
  `sentencepiece-sys-0.11.3` vendor source, with shared libraries, tests and
  tcmalloc disabled; CMake compatibility minimum 3.5. It is a build dependency
  of the shared native library; this test does not invoke an ASR tokenizer.
- CPU means CPU plus Apple's Accelerate backend; Metal includes CPU fallback
  for unsupported operations. `doctor` reports Metal GPU device 0, Apple M5 Pro,
  plus Accelerate and CPU. Verbose confirmation records CPU device -1 and GPU
  device 0; source rejects an unavailable requested GPU instead of silently
  selecting CPU. Per-operator GPU placement was not profiled.
- Default streaming geometry: chunk 13, right context 1, left context 0,
  FIFO 80, speaker cache 264, refresh 40, all in 80 ms encoder frames.
  Buffering is 1.12 s before compute. Four scheduler CPU threads are hardcoded
  in this native source; no public standalone thread override was used.
  One recording at a time, no dynamic batching. Thresholds: onset .641,
  offset .561, padding .229/.079 s, minimum on/off .511/.296 s. No tuning.

The script uses standard-library Python through the repository's locked uv
project. Given a pinned native checkout and existing corpus downloads:

```sh
# From the repository containing the ignored public corpus cache:
DIAR_CACHE="$PWD/.cache/benchmarks/nemotron3-diarization-2026-09-25"
# Before configuring, build/provide SentencePiece and initialize ggml.
cd "$DIAR_CACHE/NeMo-Speech.cpp"
scripts/configure.sh metal-diar -G 'Unix Makefiles' \
  -DNEMO_SPEECH_BUILD_MIC_CAPTURE=OFF \
  -DSENTENCEPIECE_LIB="$DIAR_CACHE/sentencepiece-build/src/libsentencepiece.a" \
  -DSENTENCEPIECE_INCLUDE_DIR="/path/to/pinned/sentencepiece/source/src"
cmake --build build/metal-diar -j 8
cd -
uv run --locked python scripts/probe-nemotron-diarization.py --self-test
uv run --locked python scripts/probe-nemotron-diarization.py \
  --cache "$DIAR_CACHE" --prepare
uv run --locked python scripts/probe-nemotron-diarization.py \
  --cache "$DIAR_CACHE" --backend cpu
uv run --locked python scripts/probe-nemotron-diarization.py \
  --cache "$DIAR_CACHE" --backend metal
# One confirmation per backend; never overwrite the earlier results:
uv run --locked python scripts/probe-nemotron-diarization.py \
  --cache "$DIAR_CACHE" --backend metal --case ami-600-645 \
  --run-name confirmation --verbose
```

The script verifies model/audio hashes and source HEAD, refuses existing result
directories, preserves stdout/stderr/RTTM and failed validation rows, and kills
the process group after a 300-second timeout. Its report-writing exit code is
not a quality gate: inspect every row's status. All inference uses a local
model path and local audio. Audio, weights, builds and generated JSON stay
ignored; no personal recording was used.

### Corpus provenance

[AMI](https://groups.inf.ed.ac.uk/ami/download/) ES2004a Mix-Headset and manual
annotations 1.6.2 were already available locally. Original distribution is
CC BY 4.0; see the existing [AMI source manifest](../../benchmarks/asr/manifests/audio-regression-2026-09-16/ami-windows.json)
for original and mirror URLs. Mirror revision:
`722d8891643e1e4dc62cfd0d198fa05a1646c3cc`. Source audio SHA-256
`3e2560b19bee6952c7c7ce041b0f1ea8a7ea9468044c4eea79d2a2c67e24ab0f`;
annotation ZIP `56e29112cd89cfba76fbf8f10215eb245cc89b0d3d27355ba14b406a7ea7776c`.
Extract the fixed windows as mono 16 kHz PCM16; do not select windows based on
model output. No training-set exclusion or statistically held-out claim.

[Google FLEURS](https://huggingface.co/datasets/google/fleurs/tree/70bb2e84b976b7e960aa89f1c648e09c59f894dd)
test en-US `11974497390699501411`, CC BY 4.0, uses the existing clean fixture
SHA-256 `ff3f24289273e664640d6b00387a81aa135e61e4b62095ffd16a4b9819c92d5a`.
It was converted from float WAV to PCM16 without changing rate or duration.
Silence is 160,000 exact-zero mono PCM16 samples at 16 kHz. Derived WAV hashes:

| Sample       | SHA-256                                                            |
| ------------ | ------------------------------------------------------------------ |
| AMI 600–645  | `6b623d3d09bfd3044ebd8e5b825dee0d49a6723c74b930859d0e3c7483a494c0` |
| AMI 675–705  | `de8ceba48b4659a2269ff32a39578754d21a11427291afdacc2ac821aba12e34` |
| FLEURS PCM16 | `6d16ef2f8527820892ec01bc8251771fdf617e0bff2cb3b5b0ed8ba8e73e5f67` |
| Silence      | `ee7bea4232762775f8fce9b3e27e4d3948c8ac6a45a87ca769f703d6eed0b448` |

An initial preparation attempt failed because Python's WAV reader rejected the
existing floating-point FLEURS fixture; a subsequent launch stopped before
inference because that incomplete plan did not exist. Both are setup failures,
separate from the ten actual native attempts. The preparation correction is
explicit PCM16 conversion; the partial corpus remains preserved in the cache.

## Application integration validation

The user subsequently approved [ADR-0022](../adrs/0022-local-speaker-timelines.md).
Validation used `feature/local-speaker-timelines`, based on `8a2c27b`, with the
implementation/test snapshot identified below. Local commits and integration
are recorded in issue #42; these results do not imply a published release.
That initial integration was experimental and false by default; the subsequent
automatic-policy validation below supersedes its setting behavior. All inputs below
are the same public/generated WAVs above; no microphone or private transcript was
used. Inference used only local files, without an API or inference-time download.
The ignored 28-file implementation/test source-hash manifest has SHA-256
`a8d3360e9f735433e0784884aff3814699916e2952992c9099b8a563151a8860`;
documentation is excluded to avoid a self-referential digest.

Commit preparation added one scanner annotation beside a public dylib filename
in `scripts/package-nemotron-diarization.py`, which secret scanning had mistaken
for an API key. Its parsed Python syntax remains identical to the tested version;
all other implementation/test files remain byte-identical to the manifest.
The exception applies only to that filename; secret scanning stays enabled.

### Correctness and application behavior

- 254 Rust unit/regression tests passed across core, data, desktop, inference,
  models and native bindings; 33 explicit environment-dependent tests were
  skipped in that ordinary run. Two of those real-model test functions were then
  invoked explicitly, with the worker test in two independent processes.
- The native worker completed all 16 measured sample/backend/load-order cases.
  Both fresh-process orders, ASR then diarization and diarization then ASR,
  preserved identical before/after Nemotron ASR text. Fresh recording state,
  cancellation before loading, cancellation during packet delivery and reuse
  after cancellation passed. Each meeting returned four speakers, FLEURS one
  and silence none. The 45-second timeline retained its overlap and counted one
  bounded end adjustment; no output interval exceeded the audio duration.
- The desktop `WorkContext::process_file` path passed four ASR-off/on pairs:
  Cohere/FLEURS and Nemotron/FLEURS, AMI 30 s and AMI 45 s. Raw text was identical
  within every successful pair. Timelines survived store reopen and JSONL export;
  exports did not gain an approved ASR target. This exercises actual native
  models and application persistence, not mocked inference.
- Collection off returned a timeline with no session or stored audio. Cohere,
  diarization and Qwen3.5 cleanup completed together on FLEURS, retaining raw
  words and producing nonempty cleaned text. Existing automatic text memory may
  create an empty audio directory; absence of recordings, rather than absence of
  that directory, is the tested privacy invariant.
- Deterministic tests reject invalid IDs, nonfinite/invalid intervals and larger
  tail overruns; preserve overlapping ranges and legacy records; enforce
  raw-before-timeline storage; preserve raw on failure/cancellation; and reject
  corrupt, missing, duplicated, path-escaping or symlinked runtime files.
  The 64-bit C layout is checked against the pinned header.
- Frontend checks cover default-off migration, boolean validation and persistence,
  unchanged editable text, overlapping ranges in the latest result and History,
  and explicit settings save. All 18 config tests and all 51 scenarios in the
  complete browser suite passed. Svelte check reported zero warnings/errors;
  the production web build, scoped ESLint, all-target Clippy with warnings denied,
  Rust formatting and Python lint/format/scorer checks passed.
  Native commands are mocked in browser tests; native execution is covered above.

### Preserved failures and limits

**Cohere did not complete AMI 675–705.** Its fourth internal segment reached
`duration_token_limit` before diarization began. The failure reproduced with the
speaker setting both false and true. The integration does not bypass the native
completion guard or label incomplete words as a successful transcript. These two
failed ASR attempts remain explicit controls in the final 12-call desktop matrix:
eight successful paired calls, two successful collection-off calls, two failed
Cohere meeting controls. Successful Nemotron meeting tests do not qualify Cohere
on this input. The earlier all-Cohere desktop run stopped at this same failure;
its successful FLEURS pair and failed meeting are retained separately.

An initial coexistence attempt stopped at ASR artifact verification because the
isolated root contained the Metal release, not the separate CPU ASR archive.
The corrected fixture explicitly selects that installed Metal ASR package and
still runs diarization on both CPU and Metal. No inference result was produced
by the failed setup attempt. An intermediate desktop test wrongly required the
collection-off audio directory not to exist; it was corrected to require zero
sessions and zero audio files. No private audio had been written.

The browser fixture for model status inherited `ready: true` on deliberately
missing files. It now sets readiness consistently with its fixture's installed
state; the production fallback-ready behavior was retained. One intermediate
agent-browser full run stalled after unrelated animation/History timing failures;
focused checks and the normal fresh Chromium regression runner were rerun.
Default-feature Clippy also exposed a pre-existing cfg-dependent always-false
expression in model selection; the equivalent cfg-separated expression passes
without suppressing warnings. The example pipeline's new optional output fields
were added so all-target compilation succeeds.

No standard DER requalification, reference WER/CER comparison, owner recognition,
word-to-speaker alignment, language/accent coverage, forced Automatic GPU failure,
long-session/capture-stream test or sustained power/thermal test was performed.
Raw-text equality proves preservation for these inputs, not better word accuracy.
The earlier coarse DER and raw CLI boundary failures remain unchanged.

### Timings from the integrated path

The following final worker measurements are wall time around the bounded worker
request, including full artifact verification, model/stream creation, processing,
validation and teardown. Each diarization model is freshly loaded per job; ASR
weights are retained by the existing worker. One recording runs at a time, with
320 ms non-overlapping packets, the native default geometry/thresholds above and
one finalization. No build or browser regression ran concurrently with this final
matrix. Filesystem/shader caches were not cleared. Rust uses its debug test
profile; the separately compiled native bundle uses Release. These are neither
microphone stop-to-final nor algorithmic latency measurements.

| Sample         | CPU, ASR loaded first | CPU, diarizer loaded first | Metal, ASR loaded first | Metal, diarizer loaded first |
| -------------- | --------------------: | -------------------------: | ----------------------: | ---------------------------: |
| AMI 45 s       |               3.865 s |                    3.844 s |                  .612 s |                       .613 s |
| AMI 30 s       |               2.127 s |                    2.130 s |                  .395 s |                       .396 s |
| FLEURS 4.62 s  |                .177 s |                     .174 s |                  .102 s |                       .102 s |
| Silence 10 s   |                .402 s |                     .393 s |                  .155 s |                       .160 s |
| Four-case mean |               1.643 s |                    1.635 s |                  .316 s |                       .318 s |

The initial successful worker run, before independent-process order checks, had
a **21.124 s** first CPU meeting result; its subsequent CPU repeat was 3.836 s.
That cold/system outlier is retained, not replaced by the final table. Causes
were not isolated. There are too few independent observations for p95 or stable
cold-latency claims. CPU/Metal averages are not combined with each other or with
the earlier standalone CLI measurements.

| Desktop successful pair   | ASR-only complete-file path | With speakers | Measured speaker stage |
| ------------------------- | --------------------------: | ------------: | ---------------------: |
| Cohere CPU / FLEURS       |                     3.188 s |       2.987 s |            2.701 s CPU |
| Nemotron Metal / FLEURS   |                      .498 s |        .181 s |           .100 s Metal |
| Nemotron Metal / AMI 30 s |                      .366 s |        .753 s |           .397 s Metal |
| Nemotron Metal / AMI 45 s |                      .549 s |       1.137 s |           .605 s Metal |

Desktop times surround `process_file`: decode, ASR, optional speakers,
personalization and persistence; export assertions run afterwards. Writing
cleanup is disabled in these timing rows and tested separately. The baseline
always ran first: different resident/model/shader state makes simple subtraction
invalid as an estimate of diarization overhead. In particular the FLEURS pairs
do not show diarization accelerating ASR. The measured stage is the relevant
observation, and its first CPU initialization cost differs from later worker
rows. Integrated process RSS/VRAM, first-speaker/endpoint timing and power were
not instrumented; earlier standalone RSS does not describe the complete app.

### Integrated runtime identity and reproduction

The hardware, OS and diarization weights/source revisions are as recorded above.
The seven renamed dylibs are loaded with local symbol scope and linked only to
their namespaced closure and Apple system libraries. The tested local manifest
SHA-256 is `553147502c6eb918aba6d8c7f52d6ec43b5459da695dbaa55997a317e61d29f6`.
It records generated local provenance, not a publisher attestation.

| Library component (`libvesper_diar_97a15afa_*.dylib`) | SHA-256                                                            |
| ----------------------------------------------------- | ------------------------------------------------------------------ |
| api                                                   | `6d2e8b1afb4b57c64182300a98036085834a7eebcd5a4027cdec51aa8ec9273c` |
| asr                                                   | `a58cdc98d9a163d30b4e8c3f8a1ceb3887a2669b2f7a147af10a26e1dd48201d` |
| ggml                                                  | `bd074a03827f34457e8b87c0dc56da3def7cc45a0ec7d66c5b615b5c90cf82b9` |
| base                                                  | `7b9030b9eecedfb0354f469cc27860e0909717a74e0ab465b204f3df784f8774` |
| cpu                                                   | `9e125716be982e2c95e293b0ecf47480bf24d0ae8ae7acfe652fc08aaf8d1798` |
| blas                                                  | `4b58309b87ea57de123c6be8dcbb23ca2bbe693d0f9d3708d9838efaba2a75ab` |
| metal                                                 | `d105b3eefdfd9b69a24f1247eeeda0313ad290a33ed1e735400a1626db26a15b` |

ASR uses the existing pinned `models/artifacts.json`: Nemotron 3.5 Q8_0 model
revision `ea30d66debe3740a08b573244286791d423d6b3e` with v0.1.0 macOS arm64 Metal
runtime revision `4f9676226f667d14608487df744f375db87127f8`; Cohere INT8 ONNX export
revision `156a470cf08eefe706a0004f3c52d9ee567ca7a0` with the repository's verified
Sherpa 1.13.8 completion-reporting runtime, CPU provider and eight graph threads.
Actual artifact hashes and execution/provider records remain in the ignored
test logs. Qwen3.5 0.8B uses the pinned Q4_0 artifact and llama-cpp-2 0.1.156 on
CPU for the cleanup coexistence assertion. No ASR or writing weights changed.

Follow the [development commands](../development.md#experimental-speaker-runtime)
to package and run the three explicit native invocations. Ordinary checks are
`cargo test --locked --offline` for the six changed crates' libraries,
`cargo clippy --locked --offline ... --all-targets -- -D warnings`, config Node
tests, Svelte check/build, ESLint and the existing Playwright recovery suite.
The probe's seven analytical scorer checks and Python lint/format checks are
independent of application inference. Generated logs, binary manifests, models
and audio remain ignored; this Markdown is the durable reviewed evidence.

## Automatic-policy validation — 26 September 2026

At the owner's request, the follow-up removes the speaker switch and retired
preference. Both application entry points admit analysis automatically when local
components and the backend are supported. Missing/partial components skip it;
present but invalid components still undergo full verification and preserve raw
text on failure. [ADR-0022](../adrs/0022-local-speaker-timelines.md) records the
original stage design and opt-in decision; this follow-up and the
[user guide](../user-guide.md#experimental-speaker-timelines) describe the current
automatic behavior. The original ADR has not been amended.
This policy change is based on `cb5e7a6`; its signed local commit series is recorded
in issue #42. It does not change models, native libraries or their lifetimes.

The following checks passed on the same Mac and pinned local artifacts/corpus
described above:

- 194 deterministic Rust tests: core 41, desktop 66, inference 61 and models 26.
  The ordinary run skipped 32 environment-dependent tests; the real desktop
  function was then invoked explicitly. New coverage includes retired true/false
  settings, incomplete bundles, unsupported backends, corrupt installed files,
  cancellation and raw-text preservation. ASR installation remains independent
  of speaker components.
- `make check-fast`: 127 Node tests plus lock consistency, formatting, language
  lint, version checks and Svelte diagnostics (zero errors/warnings).
- All 51 mocked-native browser scenarios passed in the normal Chromium runner
  and an isolated headless agent-browser session. The speaker scenario loads an
  old false setting, verifies automatic latest/History timelines and unchanged
  editable text, and requires neither a toggle nor a settings save.
- Workspace/all-target Clippy with warnings denied and the production web build
  passed. No generated JSON, audio, model or build output is part of this change.

The real `WorkContext::process_file` test passed with existing disabled settings
retired. Four direct ASR-worker baselines and their four automatic desktop runs
had identical raw text: Cohere/FLEURS and Nemotron/FLEURS, AMI 30 s and AMI 45 s.
Speaker counts were 1, 1, 4 and 4; the last case recorded the existing one-frame
tail adjustment. Timelines survived reopening and dataset export without becoming
approved ASR labels. Two further desktop runs verified collection-off behavior
and coexistence with Qwen cleanup. These are ten successful calls plus the two
retained Cohere meeting failures, which again stopped at `duration_token_limit`
before diarization for either retired preference value.

The direct-worker baseline excludes desktop persistence; its
`decodeAndAsrSeconds` must not be compared as equivalent to the earlier full
desktop off/on timing column. This follow-up is a correctness rerun, not a new
isolated latency, DER or WER qualification. Broader language, speaker, sustained
resource and live-capture limits above remain open.

Intermediate failures were preserved during validation: an invalid test fixture
paired sherpa ASR with Vulkan before reaching speaker admission; it now uses a
configuration-valid Nemotron/Vulkan control. A concurrent older-worktree build
sharing the Cargo target directory left stale native-binding metadata; refreshing
that dependency allowed compilation. The sandbox blocked the model test's local
HTTP listener and Metal command-queue creation; reruns with the necessary local
access passed. None of those incomplete attempts count as successful native runs.
