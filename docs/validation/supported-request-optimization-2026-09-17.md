# Shared inference request optimization — 17 September 2026

> Historical evidence: names and commands below describe the recorded revision.
> For the current checkout, use the corresponding `vesper-*` package/executable
> and `VESPER_*` override names; legacy environment aliases remain supported.
> See [current naming and compatibility](../development.md#product-name-and-compatibility). No measurement has been requalified by renaming.


The shared request-path changes preserve output on **43 baseline/candidate
pairs**, plus three accelerated/real-time final-text comparisons. The executed
matrix contains **89 evaluations**: 80 completed-audio ASR→writer evaluations and
nine streaming evaluations. Latency changes are small and mixed; this experiment
does **not** establish a material end-to-end speedup. The principal improvements
are avoiding redundant copies/calls and stopping cancelled work sooner at
application boundaries.

The transcript-free local debug report at `.cache/benchmarks/results/supported-request-optimization-2026-09-17/debug-summary.json`
contains per-case timings, score counts, output hashes, exact artifact/runtime
provenance, source inventories, process RSS and failures. The earlier
[hardware SHA result](application-sha-2026-09-17.md) is a separate change; its
roughly 51% reduction in the measured Cohere→writer stage sum must not be
attributed to this batch.

## Implemented behavior and research boundary

- The manager checks cancellation before copying/submitting requests and before
  replacing resident handles. An already-cancelled request cannot evict the
  selected resident speech model while attempting to select another one.
- Streaming has a separate cancellation flag. Cancel/drop bypasses queued PCM,
  wakes an idle receiver, and is checked around native push, polling, Finish and
  result callbacks. Stream-only cancellation leaves the application operation
  usable for complete-recording fallback. Cancellation from a final-result
  callback now returns cancellation instead of reporting success.
- Capture mixes directly from pending input, avoiding the temporary interleaved
  packet allocation. At 16 kHz, conversion transfers its owned mono buffer
  directly instead of cloning through the resampler bypass. Arithmetic order,
  other sample-rate conversions, archive samples, packet sizes and tails are
  preserved.
- Qwen writing removes the extra sampler `accept` after `sample`. The pinned
  [llama-cpp-2 0.1.156 API](https://docs.rs/llama-cpp-2/0.1.156/llama_cpp_2/sampling/struct.LlamaSampler.html#method.sample)
  already samples and accepts. This saves one FFI call per generated token;
  the current greedy sampler is stateless.

**Paper evidence:** [Stateful Conformer with Cache-based Inference, v3](https://arxiv.org/abs/2312.17279v3)
supports bounded native state and avoiding overlapping recomputation for a
compatible trained streaming model. Its results do not quantify these Rust
changes. **Official runtime capability:** the
[NeMo-Speech.cpp 0.1.0 SDK](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.1.0/docs/sdk.md)
defines recognizer/stream ownership and push/poll/finish/drain. Our cancellation
flag operates around those synchronous calls; it cannot interrupt a native call
already executing. **Local evidence** is reported below. **Hypothesis:** fewer
copies/calls and earlier cancellation should reduce wasted work under contention;
there is no measured service-load or battery benefit yet.

This is category 1, frozen-weight application/public-API work under the
[paper-to-engineering map](../research-streaming-inference-papers.md). Category 2
kernel/decoder changes and category 3 checkpoint/training changes were not
implemented. Existing model lifetimes, quantizations, decoder/context settings,
adapter selection and disabled-by-default prefix reuse are unchanged.

## Paired CPU protocol

- Apple M5 Pro, 18 logical CPUs, 64 GiB RAM, macOS 26.6.2 (25G83), CPU, battery
  power at every measured process start; power mode 0. Unrelated system load and
  thermal behavior are uncontrolled. OS caches were not flushed.
- Rust 1.98.1 release builds. Baseline executable SHA-256
  `ac07d2f9fab1ab38ea365fd64c4c1c74cb1cf4005a1828c0b5565d76d5582ec2`;
  candidate `adb410d9c2474e7d83d1c89f3cfc290b8e78f16249199a05f905febb7807faac`.
  Both include the prior hardware SHA optimization. The source inventories differ
  only in `manager.rs`, `qwen.rs`, audio `pcm.rs` and `capture.rs`.
- sherpa-onnx 1.13.8, upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`,
  patched build `48de319e110df610b6d53933f47f5669f3cf18b837c5974d2c40d21668586bf5`;
  llama-cpp-2 0.1.156 / llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621`;
  Nemotron uses its pinned NeMo-Speech.cpp 0.1.0 CPU archive. Exact weight,
  projector and runtime file hashes/revisions are in the JSON.
- Explicit CPU; ASR adapter native threads up to eight (Nemotron reports four),
  writer up to eight. OMP/BLAS/Accelerate thread environment caps are one;
  inherited application/GGML/llama overrides removed; prefix experiment off.
- Five ASRs: Cohere Transcribe INT8, Qwen3-ASR 0.6B INT8, Parakeet v3 INT8,
  Nemotron 3.5 0.6B Q8 and LFM2.5 Audio 1.5B. Each uses the installed Qwen3.5
  0.8B Q4_0 writer. Models are timed sequentially, without concurrent compilation.
- Eight identical completed recordings/model/build: six distinct natural English
  FLEURS cases, one synthetic negation/amount control and one reset. Durations
  4.62–19.8 seconds include the existing long-audio segmentation path. These are
  reused regression fixtures from the [hard regression](audio-regression-2026-09-16.md),
  not a new language/noise qualification or a model-ranking corpus.
- Build order alternates by adapter. Nemotron/LFM and the writer retain weights
  after first use; Cohere/Qwen-ASR/Parakeet still load per job. The runner's
  `residentRun` field must not be interpreted as ASR residency for those adapters.

ASR timing covers the actual manager call, including verification/load when
applicable, segmentation and decoding. Writer timing covers rewriting and the
fidelity gate, including first-use verification/load. Their sum excludes audio
decode, capture, archiving, storage and UI. Medians below are descriptive over
eight variable-duration requests, with no confidence interval or p95 claim.

| ASR→Qwen writer | ASR median, baseline→candidate | ASR+writer median, baseline→candidate | Writer rejections, each build |
| --- | ---: | ---: | ---: |
| Cohere | 2.976→2.931 s | 3.525→3.534 s | 0/8 |
| Qwen3-ASR | 1.673→1.658 s | 2.294→2.260 s | 1/8 |
| Parakeet | .854→.849 s | 1.300→1.293 s | 0/8 |
| Nemotron | .498→.501 s | .919→.917 s | 0/8 |
| LFM2.5 Audio | .446→.447 s | .863→.868 s | 1/8 |

All 40 completed-audio pairs preserve raw/cleaned/personalized text, WER/CER
scores and error outcomes exactly. Qwen-ASR and LFM each retain one writing
fidelity rejection in **both** builds; those cases remain in the denominator
with raw ASR preserved. No runtime inference failure occurred in the final
executed matrix. Six distinct natural cases contain 174 reference words; raw
word-edit counts remain Cohere 23, Qwen-ASR 13, Parakeet 20, Nemotron 18 and LFM
12. Equality is a regression check, not proof that these outputs are correct.
The synthetic meaning control adds narrow coverage; no new human meaning review
or general intent-quality claim follows.

Candidate whole-process peak RSS: Cohere 5.707 GB, Qwen-ASR 3.139 GB, Parakeet
2.407 GB, Nemotron 2.407 GB and LFM 3.031 GB, including writer/harness. Baseline
peaks are in the JSON and similar. These are decimal GB, not idle model memory.

## Continuous speech and lifecycle checks

Nemotron receives non-overlapping 20 ms packets on a 4.62 s reference, a
**180 s stitched continuous recording**, and a fresh 4.62 s reset. Its artifact
defaults remain 56 left/3 right context frames and nominal 320 ms encoder
advance. No VAD endpoint policy was introduced. Three baseline/candidate
accelerated pairs match exactly; the candidate's real-time replay also matches
all three accelerated final texts and has no inference errors.

| Candidate paced case | First nonempty partial | EOF/Stop-to-final | Partial events | Partial edit-distance sum |
| --- | ---: | ---: | ---: | ---: |
| First request, model cold | 1924.3 ms | 58.4 ms | 152 | 12 |
| 180 s, resident model | 1628.9 ms | 57.7 ms | 8921 | 387 |
| Reference after reset | 1638.0 ms | 56.9 ms | 152 | 12 |

First partial is measured from stream-start request; Stop-to-final starts after
the last paced packet delivery and ends when Finish returns. The 180 s ASR call
took 180.377 s, including replay pacing; its near-one RTF is not compute RTF.
Partial edit-distance counts include additions/revisions and are not UPWR/UPSR.
Streaming process peak RSS was 1.071 GB. Word-emission timestamps, automatic
endpoint delay, capture queue-drop counts, device microphone latency, idle
memory, energy and sustained thermal drift were **not measured**. The harness
backpressures its producer, so zero inference errors is not evidence of zero
microphone drops.

Passed **56 focused unit tests** (18 audio, 38 inference), strict all-target
audio/inference/eval Clippy, Cargo formatting, desktop Cargo check and runner
Ruff/compile checks. Three separately invoked real-runtime tests also passed:

- Nemotron consecutive fresh streams, contextual-state isolation, midstream
  cancellation followed by complete-audio fallback, final-callback cancellation
  followed by a fresh successful request, operation IDs and final-tail delivery.
- LFM A→B→A, output-limit rejection, silence/rate handling, cancellation/reset,
  between-segment cancellation, two-stage writing and unload/reload. With a
  nominal 50 ms cancellation signal, this debug-build request returned after
  129.7 ms from submission; it does not demonstrate native-call interruption.
- Qwen long-dictation technical-term correction through the normal two-pass
  cleanup/fidelity path.

The remaining eight opt-in inference tests were not invoked for this batch.
The real-runtime tests ran separately from release timing and used public audio.
No microphone recording or UI interaction was used.

Five runner aggregation checks also passed using captured reports with mocked
process execution: matching quality rejections remain accepted comparison data;
matching native failures, missing reports, nonzero processes and empty paced
results each fail the run. These are harness checks, not additional model runs.

## Reproduction and preserved unsuccessful attempts

Build and freeze `transcribe-eval` release binaries as `binaries/baseline-eval`
and `binaries/candidate-eval` in a fresh output directory. Record source hashes
and populate `environment.json` for the actual machine. Prepare the pinned
hard-regression corpus and model files first. The
[runner](../../scripts/check-supported-inference.py) performs no downloads and
refuses to overwrite its `runs/` directory:

```sh
python3 scripts/check-supported-inference.py OUTPUT_ROOT \
  .cache/benchmarks/results/audio-regression-2026-09-16
```

Nemotron uses `.cache/benchmarks/model-roots/nemotron-cpu`; other adapters use
`models/downloads`. An isolated follow-up may specify
`--models parakeet --offline-only` with the same frozen binaries and corpus.
Native logs are decoded with replacement for invalid UTF-8; reports retain
machine-readable failures independently of logs. A nonzero process or missing
report makes the runner fail. Matching quality rejections are still counted.

Raw evidence roots are `.cache/benchmarks/results/` followed by:

- `supported-optimization-2026-09-17`: incomplete initial attempt, preserved.
  Four process readiness failures (both Parakeet and both Nemotron variants);
  40 reported rows, including an LFM report whose process exit was not recorded
  because the runner crashed decoding native log bytes. Its candidate preceded
  the final polling/callback cancellation fix. Excluded from the final comparison.
- `supported-optimization-2026-09-17-v2`: final binaries, explicit Nemotron CPU
  root and corrected log decoding. 73 executed rows; both Parakeet processes
  failed readiness, leaving 16 planned cases unexecuted. The runner correctly
  exits nonzero and preserves them as missing.
- `supported-optimization-2026-09-17-parakeet`: after downloading and verifying
  the four repository-pinned Parakeet artifacts at
  `2bda32ec70b097a55adaa07d9a7173915b43cc78`, both frozen builds execute the
  missing 16 cases successfully. This is separate follow-up evidence; it does
  not rewrite either failed attempt.

The published aggregate combines the final 73+16 executed cases while retaining
the original failure ledger. Kyutai's weights/sidecar are absent and were not
benchmarked. Gemma 4 remains a separate pre-integration candidate. Other GGUF
writers, languages, automatic-language/code-switching/singing, accelerators and
edge devices are not qualified by this CPU English regression subset.

At this shared-fix checkpoint, the [three-adapter residency proposal](https://github.com/rahulsharmavishwakarma/transcribe/issues/2#issuecomment-5716874721)
was pending explicit approval. Cohere alone was subsequently approved and
[implemented and qualified separately](cohere-residency-2026-09-17.md), including
identity/invalidation, 100-cycle isolation, memory and paired-corpus checks.
Qwen-ASR/Parakeet residency remains unapproved. Issue #2 stays open for its
broader lifecycle/device/full-request gates.
