# Application verification optimization — 17 September 2026

The actual Cohere→Qwen request path benefits substantially from enabling the existing SHA library's Apple ARM hardware backend. Across 30 paired completed recordings per build, median ASR time falls **7.246→3.319 seconds (54.2% lower)** and ASR plus writing falls **7.718→3.793 seconds (50.9% lower)**. All paired outputs match. These are inference-stage measurements on one CPU, not microphone/Stop-to-visible latency or a general edge-device performance claim.

Implementation and qualification are tracked in [issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42). The local debug report at `.cache/benchmarks/results/application-sha-2026-09-17/debug-summary.json` contains transcript hashes, scores, failures, stage measurements, artifact/runtime pins, source inventories, binary hashes and process envelopes. Raw public-audio reports remain under the same ignored run directory.

## Change and evidence boundary

`transcribe-models` enables `sha2/asm` only for `aarch64-apple-darwin` through its target-specific dependency. Cargo.lock retains sha2 **0.10.9** and adds its existing optional sha2-asm **0.6.4** backend. The official SHA-256 dispatch selects its CPU-detected ARM implementation when available, with software fallback. Every artifact is still read completely and checked against its exact size and digest. No successful-verification cache, inference lifetime, model selection, prompt, decoder setting or production Rust inference function changes. [Official dispatch source](https://docs.rs/sha2/0.10.9/src/sha2/sha256.rs.html).

The desktop and evaluation feature graphs both enable this backend. Linux ARM and x86 feature-graph checks retain their existing selection; those targets were not compiled or benchmarked. This target restriction deliberately limits adoption to the tested platform. The SHA feature applies at runtime; Cargo resolver 2 keeps the inference build-script's separate SHA dependency unchanged.

This is frozen-weight/application-library work supported by the current public dependency API. [FlashAttention](https://arxiv.org/abs/2205.14135) concerns attention's memory traffic, and [Prompt Cache](https://arxiv.org/abs/2311.04934) concerns reusable prompt state; neither predicts elimination of host file-verification time. No new attention/decoder kernel or checkpoint/training technique was implemented. Their paper results remain separate from this local measurement.

## Paired protocol

- Apple M5 Pro, 18 logical CPUs, 64 GiB RAM, macOS 26.6.2 (25G83), **battery power**, power mode 0. Every process starts on battery. Thermal telemetry and unrelated system load are uncontrolled. These results are kept separate from the earlier AC-powered [isolated hash probe](audio-regression-2026-09-16.md).
- Rust 1.98.1; release profile, one codegen unit, thin LTO. Both application-linked examples were built from the same production sources and frozen before timing. Differences are the target dependency/lock selection and additional integrity tests; tests are not compiled into the examples. Complete source-file inventories are included in the JSON.
- Cohere INT8 export `156a470cf08eefe706a0004f3c52d9ee567ca7a0`, four files totaling 2,888,052,036 bytes; Qwen3.5-0.8B Q4_0 GGUF at `8fea620810c4afa23dd6443f999a48574c1611a3`. Exact hashes are in the JSON.
- sherpa-onnx 1.13.8, upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, local completion-report build `48de319e110df610b6d53933f47f5669f3cf18b837c5974d2c40d21668586bf5`; llama-cpp-2 0.1.156 / llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621`.
- Explicit CPU, ASR/writer up to eight threads; library thread variables capped at one; inherited application/GGML/llama experiment overrides removed. Qwen prefix caching explicitly off. Cohere remains per-job; Qwen weights persist within each process, with fresh contexts per pass.
- The same 24 held-out English FLEURS recordings, four synthetic meaning controls and original reset from the hard regression, plus one explicit reset: **30 attempts/build**, 316.957 seconds/build, individual recordings 3.108–23.640 seconds. This reuses the prior validation set to test an output-preserving implementation change, not to select a new model or tune thresholds.
- Two blocks of 15: software/hardware, then hardware/software. Each build has two process-cold writer requests and 28 requests with resident writer weights. Filesystem caches were not flushed; a new process is not cold storage. Timed models run sequentially with no compilation during inference.
- Five additional diagnostic cases/build separate verification, recognizer creation, waveform acceptance, decode and release. Those ten cases are excluded from the 60 pipeline attempts and corpus timing distribution.

ASR timing wraps the actual manager call, including verification, model construction, segmentation and decode. Writing wraps the actual rewrite call, including its verification/load on first use and fidelity gate. Their sum excludes capture, file decode, archiving, personalization/storage, GUI delivery and benchmark report writing. Process wall time includes harness overhead and is a separate field. Quantiles use nearest rank over this variable-duration corpus; they are not fixed-size service-request distributions.

## Results

| Metric | Software SHA | Hardware SHA |
| --- | ---: | ---: |
| ASR p50 / p95 | 7.246 / 8.014 s | **3.319 / 4.243 s** |
| ASR + writing p50 / p95 | 7.718 / 9.011 s | **3.793 / 4.890 s** |
| Resident-writer p50 / p95, 28 requests | .367 / .572 s | .380 / .632 s |
| Process-cold writer, two observations | 7.676 / 1.698 s | .843 / .853 s |
| Maximum complete-pipeline process RSS | 5.800 GB | 5.814 GB |
| Pipeline attempts / missing / runtime failures | 30 / 0 / 0 | 30 / 0 / 0 |
| Writer fidelity rejections, retained | 1 | 1 |

Cold-writer observations vary substantially with process/file/backend warm state; two observations do not establish a cold-start distribution. Resident writing did not speed up, and this is not a memory optimization. RSS uses decimal GB and covers the entire process, not just weights. No confidence interval or sustained thermal qualification is claimed from this two-block run.

| Separate five-case median | Software SHA | Hardware SHA |
| --- | ---: | ---: |
| Full artifact verification | 5.130 s | **1.035 s** |
| Recognizer construction | 1.760 s | 1.750 s |
| Waveform acceptance | 1.252 ms | 1.282 ms |
| Decode | .293 s | .297 s |
| Recognizer/stream release | 41.751 ms | 42.156 ms |

Verification improves **4.96×** in the application-linked probe; that is not a 4.96× application speedup. Model load/decode remain similar, supporting the attribution to hashing. Five diagnostic pairs match PCM, raw output, score and native completion reason, including the previously observed quiet-input failure. The change does not fix low-level audio recognition quality.

All **30 pipeline pairs** match input/output PCM, raw text, accepted cleaned text, WER/CER and error/rejection outcomes exactly. The 24 natural English recordings retain **59 edits / 610 words = 9.67% raw WER** in both builds. The writer rejection is preserved in both denominators rather than treated as a successful cleaned transcript. No denoising or filler deletion was enabled.

## Checks and reproduction

Passed 24 model-management tests, including complete-file corruption/truncation/oversize/cancellation and independent SHA-256 known answers (empty, `abc`, one million `a` bytes crossing 128 KiB read boundaries). The local HTTP download fixture initially could not bind inside the sandbox; the full suite passed on the host. Model/evaluation all-target Clippy with warnings denied, desktop Cargo check, Cargo formatting and runner Ruff checks passed. These checks do not launch or qualify the desktop UI.

Prepare the public [hard-regression corpus](../../benchmarks/asr/audio-regression.md) and pinned model files. Freeze the two release variants using the dependency snapshots in the raw evidence; change only the target SHA feature between builds, and retain both binaries before starting timing. Build each variant with:

```sh
export DEVELOPER_DIR=/Library/Developer/CommandLineTools
source scripts/dev-env.sh
cargo build --release -p transcribe-eval \
  --example audio_frontend --example cohere_stage_probe --offline --locked
```

Copy the examples into a fresh output directory's `binaries/` as `software-audio_frontend`, `hardware-audio_frontend`, `software-cohere_stage_probe` and `hardware-cohere_stage_probe`. The [runner](../../scripts/qualify-application-sha.py) expects those files and refuses to overwrite a previous or partial run:

```sh
python3 scripts/qualify-application-sha.py \
  .cache/benchmarks/results/application-sha-2026-09-17 \
  .cache/benchmarks/results/audio-regression-2026-09-16
```

Use a new output path for repetition. Populate its environment for the actual device/power envelope before comparing results; the dated runner uses this experiment's known envelope and records per-process power. Its raw summary includes public-corpus normalized text inside scores; the published JSON removes those fields and deduplicates common model provenance.

## Remaining request-path work

The strongest next proposal is [resident Cohere in the existing bounded worker](https://github.com/rahulsharmavishwakarma/transcribe/issues/2#issuecomment-5716242828), with full verification on model load, typed artifact/runtime identity, fresh segment streams and measured invalidation/reset/memory behavior. At the time of this SHA experiment, it remained pending explicit architecture approval. The subsequently approved [Cohere residency qualification](cohere-residency-2026-09-17.md) records that separate implementation and its measurements.

The audit also identified frequent live-status filesystem reads, complete-capture resampling after successful native streaming, and repeated database connection/schema setup. Current automatic learning depends on the resampled-audio identity, and raw-before-cleanup persistence is intentional; changing those flows needs explicit preservation of their contracts. Add complete request/Stop-to-visible timings before optimizing them. Native streaming queues/cancellation, non-Apple CPUs/edge devices, other backends and sustained power/thermal behavior are outside this focused SHA qualification.
