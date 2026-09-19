# Cleanup inference screening

The [native-learning cleanup follow-up](../../docs/personalization/evaluation.md#cleanup-reliability-follow-up) separately investigates instruction echoes and formatting-policy violations. Two replacement prompts were rejected as primary prompts. The [bounded recovery](../../docs/personalization/evaluation.md#bounded-formatting-recovery) retains the original first pass, restores source contractions conservatively and permits one qualified short retry; development and disjoint qualification each have 58 text cases. [Reproduction](../../docs/personalization/native-qualification.md#reproduce-the-diagnostic-runs) retains both rejected experiments and the production comparison. This does not change the prefix-cache experiment below.

This development experiment preserves the sequential technical-tail and full-cleanup passes. It measures whether reusing complete immutable prompt-prefix state improves the pinned Qwen3.5 runtime. It is not an ASR benchmark, SOTA comparison, or estimate of microphone-to-visible-text latency.

## Recorded CPU result

The full local debug report with 60 samples and source digests remains at `.cache/benchmarks/results/2026-09-15-m5-pro-prefix-screen/debug-summary.json`: Apple M5 Pro, 18 logical CPUs, 64 GiB RAM, explicit CPU, 8 inference threads, macOS 26.6.2, AC power with low power mode off. Qwen3.5-0.8B Q4_0, llama-cpp-2 0.1.156 / llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621`, 4096-token fresh contexts, batch 512, microbatch 128, greedy decoding. Rust test/dev profile with bundled native runtime.

| Warm resident configuration | Runs | Median complete rewrite | Observed P90 |
| --- | ---: | ---: | ---: |
| Full prefill on each pass | 30 | 838.96 ms | 868.51 ms |
| Reused immutable prefixes | 30 | 366.25 ms | 378.96 ms |

Median reduction: **56.35%** on this single synthetic English technical-dictation fixture. All 60 outputs matched the accepted reference exactly, including `README.txt`. The cache retained 46,841,268 bytes (44.67 MiB); this is snapshot payload, not whole-process RAM.

The timer surrounds `Rewriter::rewrite`, including prompt construction, both sequential model passes and fidelity checks. It excludes model loading, ASR, capture, command queue, persistence and UI. Three cached warmups precede 30 pairs with alternating order. The baseline uses the same resident weights with caching disabled. This establishes a local cache comparison, not the registry's overhead relative to its pre-refactor implementation.

Separate screening compared pre-fidelity generated strings and final acceptance for English, French and Hindi, repeated requests and profile resets; it also exercised both pass caches. One short number/negation example triggered the same raw fallback in both routes: this was retained as a baseline limitation, not counted as a successful cleanup. The native ASR smoke fixture passed with phrase hints off/on/off. None of these fixtures establishes personal-vocabulary WER improvement.

No independent cold-start distribution, upstream CLI comparison, sustained thermal study, power draw, or held-out corpus is included. P90 is descriptive for this run; 30 repeated instances of one prompt do not justify a P99, language-general quality claim, or automatic device recommendation. Personalized reference prompts, other precisions/backends and imported GGUF templates remain unqualified.

## Reproduce with installed artifacts

From the repository root, after sourcing the existing toolchain environment:

```sh
source scripts/dev-env.sh
export TRANSCRIBE_SMOKE_MODELS="$PWD/models/downloads"
cargo test --locked --offline -p transcribe-inference \
  qwen::tests::hybrid_prefix_cache_matches_fresh_requests_and_profile_reset \
  -- --ignored --exact --nocapture
cargo test --locked --offline -p transcribe-inference \
  qwen::tests::measure_two_pass_prefix_reuse \
  -- --ignored --exact --nocapture
```

The test performs no model download and uses only synthetic text. Retain the JSON lines tagged `two-pass-prefix-v1`, including failures. Avoid other compute-heavy jobs during measurement. Record hardware/power, application source digest and artifact hash anew on every machine. On macOS, use an already configured and licensed developer toolchain; this test does not alter toolchain licenses.

Application experimentation is explicitly enabled before model loading with `TRANSCRIBE_EXPERIMENTAL_PREFIX_CACHE=1`. Omit it for the baseline. Caching is disabled by default, excludes custom templates and prompts containing retrieved private examples, and keeps fresh per-pass contexts. See [ADR-0012](../../docs/adrs/0012-adapter-registry-and-reviewed-personalization.md) for exact state ownership, memory limits and research sources.
