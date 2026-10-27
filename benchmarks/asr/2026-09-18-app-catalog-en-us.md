# Nine-model application-catalog English CPU screen

All nine ASR choices that were selectable in the Models menu at the time of this run used the same fixed 16-recording FLEURS US-English pack through the application's real inference adapters. The pack contains 150.56 seconds, 324 normalized reference words, and 1,619 reference graphemes. All **144/144 model executions completed**; no model failure was removed from the aggregate. Kyutai was later removed from application support in [GitHub issue #48](https://github.com/rahulsharmavishwakarma/transcribe/issues/48); its row remains here as historical evidence.

This is local measurement on an Apple M5 Pro CPU with explicit English and completed-audio delivery. Models ran sequentially after downloads and compilation finished. The local debug aggregate at `.cache/benchmarks/results/2026-09-18-app-catalog-en-us/debug-summary.json` pins the runner, corpus, raw-report hashes, artifacts, runtimes, metric definitions, environment and limitations. Audio, hypotheses and ordinary raw reports remain in the ignored benchmark cache.

| ASR choice                    | Success | Micro WER | Micro CER | Median RTF | p95 RTF | Median ASR |
| ----------------------------- | ------: | --------: | --------: | ---------: | ------: | ---------: |
| Parakeet v2 INT8              |   16/16 | **5.25%** |     3.15% |      0.122 |   0.241 |   1,024 ms |
| Parakeet v3 INT8              |   16/16 |     5.56% |     3.40% |      0.130 |   0.236 |   1,176 ms |
| Parakeet Unified English INT8 |   16/16 |     5.86% |     3.58% |      0.115 |   0.213 |   1,038 ms |
| Qwen3-ASR 0.6B INT8           |   16/16 |     6.79% |     3.52% |      0.252 |   0.428 |   2,262 ms |
| Cohere Transcribe INT8        |   16/16 |     7.72% |     4.57% |      0.058 |   0.267 |     483 ms |
| FastConformer European FP32   |   16/16 |     9.26% |     5.06% |      0.069 |   0.124 |     608 ms |
| Nemotron 3.5 Q8               |   16/16 |     9.88% |     5.00% |      0.072 |   0.100 |     671 ms |
| LFM2.5 Audio Q4               |   16/16 |    10.49% |     6.49% |  **0.048** |   0.088 | **401 ms** |
| Kyutai STT 1B                 |   16/16 |    29.63% |    18.78% |      2.250 |   2.685 |  21,462 ms |

With the existing screening ceilings of 20% WER and 1.0 median RTF, Kyutai fails this exact CPU English profile. Parakeet v2 has the lowest measured WER, while LFM has the lowest median RTF. Those are narrow findings, not universal “best model” claims. Cohere's p95 contains its first resident load; model-lifetime behavior is intentionally the behavior users receive rather than a normalized warm-decoder microbenchmark. With 16 samples, nearest-rank p95 is the maximum observation.

The first Nemotron attempt selected the Metal runtime package from the normal application model root while the run required explicit CPU. Preflight rejected it before audio execution. The preserved successful report uses the already verified CPU runtime model root. This setup failure is documented but is not counted as an ASR execution failure.

No default changes from this screen. A defensible user recommendation still needs comparable Hindi, Hinglish, Indian-English, French, Automatic-language, streaming/live, noise, long-form and device/backend lanes. The current eight-choice Models menu may describe each current choice as locally executed for this one English CPU profile; it must not imply broader qualification.
