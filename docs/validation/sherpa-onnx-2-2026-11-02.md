# sherpa-onnx 2.0.1 runtime upgrade — 2 November 2026

Question ([#389](https://github.com/rahulsharmavishwakarma/vesper/issues/389)): does moving the patched speech runtime
from sherpa-onnx 1.13.8 to 2.0.1 keep every Sherpa-backed speech path at least as accurate and as fast, with the
completion-reporting patch behaving as before?

**Result:** Qwen3-ASR, Whisper Small and Moonshine Base gave byte-identical transcripts on all 100 English clips.
Cohere Transcribe (the default) and the Parakeet ONNX fallback changed, because 2.0 computes their NeMo-style mel
features with NeMo's additive log floor (upstream [#3999](https://github.com/k2-fsa/sherpa-onnx/pull/3999)). In
English both became significantly more accurate. In German, Parakeet lost 5 words of 1,118 (+0.45 points, at the
edge of significance) and Cohere moved within noise. Latency and memory did not change beyond run-to-run noise.

## Method

- Application: branch `chore/sherpa-onnx-2` from `main` `7926ab2`; baseline is `main` `7926ab2`, built the same way.
- Runtimes: both built by `scripts/sherpa-runtime.mjs` (static, CPU, ONNX Runtime 1.28.2 in both).
  - 1.13.8: source `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, build `0d4fb5dc410b…`.
  - 2.0.1: source `af2d8d81dbfa61c4cea4c568da78f3982edca005` (archive SHA-256 `afcafe3c34bc…`), build
    `d3b7a20612b1…`, patch SHA-256 `f936a2baa81e…`. The patch was rebased onto 2.0.1: the Moonshine v2 mask
    backport was dropped because upstream merged the same fix
    ([#3976](https://github.com/k2-fsa/sherpa-onnx/pull/3976)), and the VAD empty-range guard was kept on top of
    upstream's 64-bit sample positions ([#4079](https://github.com/k2-fsa/sherpa-onnx/pull/4079)).
- Models: the pinned artifacts in `models/artifacts.json` and `models/catalog.json`, hash-checked: Cohere INT8,
  Parakeet `vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4@8e178c8a`, Qwen3-ASR 0.6B INT8, Whisper Small INT8, Moonshine
  Base, Silero VAD v6.2 and DPDFNet8.
- Runner: `vesper-eval run … --language en|de`, completed audio, environment backend `cpu` (8 threads),
  `--asr-engine standard` for the models that also have an MLX or GGUF engine. Scoring is `vesper-eval`'s normalized
  word error rate. Differences use a 2,000-resample paired clip bootstrap over clips both runtimes finished.
- Sets: `fleurs-en-us-100` (100 FLEURS en-US clips, 2,183 words, manifest `8dd6cabe…`) and `fleurs-de-de-50`
  (50 FLEURS de-DE clips, 1,118 words, manifest `7a13c048…`), the held-out manifests of
  [#251](parakeet-weight-only-2026-10-21.md).
- Machine: Apple M5 Pro, 18 logical CPUs, 64 GiB, macOS 27.0.1, on battery with Low Power Mode off.

Each runtime is deterministic: repeated Cohere runs matched 98/100 (1.13.8) and 97/100 (2.0.1) raw transcripts and
the same failed clips, and a rebuilt 2.0.1 evaluator matched its first run 100/100. The 1.13.8 Parakeet result
reproduces the [22 October record](parakeet-unified-gguf-2026-10-22.md) exactly (5.86%, 255 ms median).

## Results

| Path                         | Set       | WER 1.13.8 | WER 2.0.1 | Difference [95% CI]  | Same transcript | Median ASR 1.13.8 → 2.0.1 |
| ---------------------------- | --------- | ---------: | --------: | -------------------- | --------------: | ------------------------: |
| Cohere (default)             | en-US 100 |      6.71% | **5.55%** | −1.16 [−1.90, −0.48] |         58 / 96 |              473 → 469 ms |
| Parakeet ONNX                | en-US 100 |      5.86% | **4.90%** | −0.96 [−1.69, −0.40] |        64 / 100 |              255 → 268 ms |
| Parakeet + Silero + DPDFNet8 | en-US 100 |      5.77% |     5.31% | −0.46 [−1.21, +0.38] |        56 / 100 |            1899 → 1917 ms |
| Qwen3-ASR                    | en-US 100 |      7.15% |     7.15% | 0                    |       100 / 100 |              494 → 500 ms |
| Whisper Small                | en-US 100 |      6.18% |     6.18% | 0                    |       100 / 100 |              597 → 624 ms |
| Moonshine Base               | en-US 100 |      9.99% |     9.99% | 0                    |       100 / 100 |                86 → 85 ms |
| Parakeet ONNX                | de-DE 50  |      6.62% |     7.07% | +0.45 [0.00, +1.05]  |         37 / 50 |              346 → 348 ms |
| Cohere (default)             | de-DE 50  |      6.78% |     7.05% | +0.27 [−0.97, +1.71] |         31 / 49 |              701 → 692 ms |

Cohere stopped at its duration token limit and returned no text on 2 English clips with 1.13.8 and on 3 with 2.0.1
(one shared), and on the same 1 German clip with both; the rows above score the clips both runtimes finished. Counting
a failed clip as every word wrong, English Cohere is 9.40% versus 9.22%.

Timing differences are within run-to-run noise. Repeated runs moved Parakeet's median between 255 and 268 ms on either
runtime. Whisper looked about 4% slower when 1.13.8 always ran first; interleaving the order gave 640/704 ms for 2.0.1
against 663/718 ms for 1.13.8, so the drift is the machine warming on battery. Peak memory for the English Cohere run
was 3.46 GB on both.

The completion-report tests (`make test-native-report`) and the Rust workspace tests pass against the 2.0.1 build.

## Limits

- Only English and German were measured; other languages, long audio and noisy speech were not.
- Cohere's duration-token-limit failures are a property of the model on both runtimes, not new to 2.0.1.
- Parakeet's German result is borderline worse. The Parakeet ONNX engine is a fallback; Automatic runs Parakeet on the
  GGUF engine ([ADR-0026](../adrs/0026-automatic-speech-engine.md)), which this upgrade does not touch.
