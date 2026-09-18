# Cohere residency qualification — 17 September 2026

Retaining Cohere in the existing worker materially reduces repeated-recording
latency on this CPU. Across 30 English recordings/build, ASR+writing p50 falls
**3.279→.805 seconds (75.5% lower)**. All **43 Cohere pairs** preserve raw,
cleaned and personalized output, WER/CER and error outcomes exactly. This is an
application lifecycle improvement, not a new decoder/kernel or a SOTA claim.

[ADR-0017](../adrs/0017-resident-cohere-recognizer.md) records the specific approval,
implementation, primary research and exact native reset contract.
[Transcript-free JSON evidence](cohere-residency-2026-09-17.json) contains every
case, output hashes, source/model/runtime identities, process RSS, failures and
lifecycle observations. The [prior copy/sampler fixes](supported-request-optimization-2026-09-17.md)
and [hardware SHA change](application-sha-2026-09-17.md) are already present in
both compared builds; their gains are not attributed to residency.

## Implemented boundary

The manager retains one selected verified Cohere recognizer alongside its
writer. Each segment still has a fresh native stream, language options and
generated decoding state. Typed artifact/runtime/options identity plus observed
file changes control reuse. Every new native load performs full size/hash
verification. Cold exact-zero audio verifies without constructing a model or
retaining verification success. Config/model changes, explicit unload and
shutdown release the handles; native/identity failures discard Cohere safely on
its owner thread. Successful cancellation discards text without unnecessarily
discarding healthy weights. A simultaneous failed native completion invalidates
the handle even when cancellation is the returned error.

Direct convenience `transcribe`/`run` calls remain per-job; residency requires
`InferenceManager`. Qwen-ASR and Parakeet lifetimes were not extended. No model,
precision, language, segmentation, prompt, thread/context default, prefix-cache
default, native kernel or training change is part of this result.

## Reproducible CPU comparison

- Apple M5 Pro, 18 logical CPUs, 64 GiB RAM, macOS 26.6.2 (25G83), explicit CPU,
  battery at all eight Cohere process starts, power mode 0. Unrelated system load
  and thermal state were uncontrolled; filesystem caches were not flushed.
- Rust 1.98.1 release builds, frozen before timing. Baseline SHA-256:
  `adb410d9c2474e7d83d1c89f3cfc290b8e78f16249199a05f905febb7807faac`;
  resident candidate:
  `4b1bdd69171f668631bcce4041b256537147f2f1aedbbe1a7287a083d9b25348`.
  Exact source snapshots are included. One unrelated native desktop test changed
  in the working tree after freezing; the measured inference sources did not.
- Cohere INT8 export `156a470cf08eefe706a0004f3c52d9ee567ca7a0`, four files,
  2,888,052,036 bytes; Qwen3.5-0.8B Q4_0 writer
  `8fea620810c4afa23dd6443f999a48574c1611a3`. Full artifact hashes are in the JSON.
- Sherpa 1.13.8 upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, patched
  build `48de319e110df610b6d53933f47f5669f3cf18b837c5974d2c40d21668586bf5`;
  llama-cpp-2 0.1.156 / llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621`.
- Eight native CPU threads; OMP/BLAS/Accelerate thread environment caps one;
  inherited application/GGML/llama overrides removed; prefix experiment off.
  Timed models run sequentially with no concurrent compilation or native tests.
- The same 30 English cases used for the prior SHA regression: 24 distinct
  natural FLEURS recordings, four synthetic meaning controls and two resets,
  3.108–23.640 s. Two blocks of 15, baseline/candidate then candidate/baseline.
  Each build has two process-cold requests and 28 later requests. The resident
  load ordinal remains one in each block, and request ordinals advance 1–15.
- Separate lanes: nine English clean/quiet/restored/noise/reset diagnostics with
  writing enabled and four French completed-audio cases without writing.
  Total **86 evaluated cases**, eight successful processes, no missing rows.
  These are regression fixtures, not a newly independent model-selection corpus.

ASR spans the actual manager request including applicable verification/load and
all segments. Writing spans rewriting and its fidelity gate. Their sum excludes
audio decode, capture, archive, persistence and UI. The p50/p95 values below use
nearest rank over mixed recording lengths. This sample supports a local
comparison, not a population p95, confidence interval or cross-device estimate.

| English metric | Per-job Cohere | Resident Cohere |
| --- | ---: | ---: |
| ASR p50 / p95, all 30 | 2.912 / 3.550 s | .447 / 2.685 s |
| ASR+writer p50 / p95, all 30 | 3.279 / 4.177 s | **.805 / 3.514 s** |
| ASR p50 / p95, matched 28 later requests | 2.915 / 3.550 s | .443 / 1.150 s |
| ASR+writer p50 / p95, matched 28 later requests | 3.267 / 4.100 s | **.776 / 1.697 s** |
| Maximum process RSS | 5.748 GB | 5.750 GB |
| Inference errors / writer fidelity rejections | 0 / 1 | 0 / 1 |

The first ASR request in each resident process takes **2.685 and 2.871 s**;
the matching baseline requests take 2.709 and 2.912 s. Residency does not remove
initial loading. Candidate verification takes .929/.947 s and construction
1.466/1.511 s, paid once per block. Their evidence fields describe that model
lifetime and must not be summed again for every later request.

Cold writer timings vary: baseline 6.900/.860 s versus candidate .829/.859 s.
The first baseline pipeline therefore reaches 9.609 s. This observation is
preserved; two starts cannot establish a cold-start distribution. The matched
28-request comparison excludes both first requests in both builds and still
shows the residency gain.

French ASR p50 is 3.024→1.467 s across four mixed-duration cases, including one
cold load each. Diagnostic ASR+writer p50 is 2.658→.190 s across nine cases.
These small, distinct envelopes are not pooled into English results.

## Quality, failure and lifecycle results

All 43 paired outcomes match. The 24 distinct natural English cases retain
**59 word edits / 610 words = 9.67% WER**. The existing synthetic spoken-instruction
writer rejection remains in both denominators with raw ASR preserved. Both
quiet-speech diagnostic rows still have **100% WER**; a successful completion
report is not correct recognition. Three noise-only clips produce zero inserted
words in both builds. These results do not improve or fully qualify quiet/noisy
speech recognition or general meaning preservation.

The final real-runtime lifecycle test passed **100 mixed ASR operations** with
writer calls interleaved: A→B→A, multi-segment input, silence, pre-cancelled model
switches, between-segment/final-callback cancellation, and cancellation signalled
20 ms into an active request. Native load ordinal remains one throughout those
cycles. Additional cases pass English→French→English, real Parakeet→Cohere switch,
explicit unload/reload, shutdown, and copied-artifact corruption, removal,
replacement and modification during a request followed by successful recovery.
Installed original files are never modified by those tests.

The cold-silence check confirms no model construction. Cold verification
cancellation returns after **24.9 ms** from request start. Ten active-request
cancellations return after **497–506 ms**, despite the 20 ms signal: synchronous
native decoding cannot be immediately interrupted. An invalid native language
produces a failed report and a fresh stream subsequently returns the original
reference. Arbitrary ORT execution faults were not induced; a focused unit test
checks that cancellation cannot conceal failed native health from eviction.

In the final debug lifecycle process, ASR-only resident RSS is 4.513 GB; cycle 10
with the writer is 5.945 GB, falling to 5.321 GB by cycle 100. Later checkpoints
never exceed cycle 10, passing the predefined 128 MiB growth ceiling. RSS falls
to **1.482 GB after unload** and 1.498 GB after shutdown. These are whole-process
RSS observations, including mapped/reclaimable pages and allocator retention,
not pure model allocation, VRAM or energy. Residency increases idle RAM even
though peak pipeline RSS is similar. An earlier intermediate 100-cycle run
also passed; its log remains separate from the final build qualification.

Also passed: **71 unit tests** (29 core, 42 inference), strict all-target
core/inference/eval Clippy, desktop Cargo check, Cargo formatting and runner
Ruff/compile checks. Eight additional Qwen-ASR baseline/candidate pairs preserve
all output/error outcomes after the shared decoding-helper refactor, including
the same existing writer rejection. The existing real Nemotron streaming
reset/cancellation/fallback test passes on the final build. Ten other opt-in
inference tests were not invoked. No microphone/UI or live Cohere streaming
performance is claimed.

## Reproduction and limits

Prepare the pinned [hard-regression corpus](../../benchmarks/asr/audio-regression.md),
the existing French long-audio fixtures and model files. Record the actual
machine in `OUTPUT/environment.json`; freeze both release `transcribe-eval`
binaries under `OUTPUT/binaries/{baseline,candidate}-eval` before timing. Run:

```sh
python3 scripts/qualify-cohere-residency.py OUTPUT \
  .cache/benchmarks/results/audio-regression-2026-09-16 \
  .cache/benchmarks/datasets/long-asr-fleurs
```

The runner refuses an existing `runs/` directory, records process status/RSS,
and fails on missing or changed outcomes. Matching pre-existing model failures
remain counted evidence, not successful predictions. Raw results are under
`.cache/benchmarks/results/cohere-residency-2026-09-17/`; the Qwen regression has
the `-qwen-regression` suffix. JSON removes transcript strings while retaining
their hashes and score counts.

The ignored `cohere::tests::installed_cohere_residency_isolation_and_invalidation`
test takes `TRANSCRIBE_SMOKE_MODELS`, `TRANSCRIBE_SMOKE_AUDIO`,
`TRANSCRIBE_COHERE_AUDIO_B`, `TRANSCRIBE_COHERE_LONG_AUDIO` and
`TRANSCRIBE_COHERE_FRENCH_AUDIO`; exact public fixture hashes are in the JSON.
Run it separately from timed models. Host `ps` supplies RSS; absence must be
reported rather than inferred from model-file sizes.

Other CPUs/edge devices, accelerators, sustained battery/thermal behavior,
full Stop-to-published-output latency and arbitrary native-fault recovery remain
unqualified. The broader lifecycle issue stays open for those gates. No staging
or commits were performed.
