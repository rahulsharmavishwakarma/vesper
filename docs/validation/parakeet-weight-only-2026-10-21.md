# Parakeet v3 CPU precision: 4-bit weight-only — 21 October 2026

Question ([#251](https://github.com/rahulsharmavishwakarma/vesper/issues/251)):
does the shipped Parakeet TDT 0.6B v3 CPU export lose accuracy to quantization,
and can a different precision recover it at a similar download size?

**Result: yes. The built-in CPU artifacts now use 4-bit weight-only quantization.**

- Dynamic INT8 quantizes activations at run time and lost accuracy. Both changes
  are measured on held-out sets:
  - 94 FLEURS en-US clips: 7.70% → 5.95% WER.
  - 50 FLEURS de-DE clips: 8.23% → 6.62% WER.
- 4-bit weight-only (ONNX Runtime `MatMulNBits`, block 32, symmetric) keeps
  activations in FP32 and comes within 0.1 points of FP32.
- The download stays about the same: the encoder is 679 MB, against 652 MB.
- Median CPU decode time is about 1.4–1.5× the INT8 time.

These are local measurements on read speech; they do not qualify other
languages, spontaneous speech or other machines.

## Artifacts

|                            | Previous                                                         | Current                                                                       |
| -------------------------- | ---------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Repository                 | `csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8@2bda32ec` | `rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4@8e178c8a` |
| Precision                  | Dynamic INT8 (QUInt8 encoder)                                    | 4-bit weight-only, FP32 activations                                           |
| Encoder / decoder / joiner | 652,184,281 / 11,845,275 / 6,355,277 B                           | 679,397,339 / 47,233,762 / 3,984,870 B                                        |
| Install folder             | `parakeet-tdt-0.6b-v3/`                                          | `parakeet-tdt-0.6b-v3-wo4/`                                                   |

The current files were produced from the official FP32 export
`csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3@1a468a35` by
`scripts/quantize-onnx-weight-only.py` in the locked
`benchmarks/asr/python/onnx-quantize` environment (ONNX Runtime 1.31.0,
ONNX 1.23.2). Two runs produced byte-identical outputs. `tokens.txt` is unchanged.
The repository's `provenance.json` lists source and output hashes.
The application's sherpa-onnx (`11afbd0`) links ONNX Runtime 1.28.2, which provides
`MatMulNBits`.

## Setup

- Machine: Apple M5 Pro, macOS 27.0.1, battery power.
- Path: `vesper-eval` release build on `main` `b239c21`. The built-in path ran
  through the production installer and the resident sherpa adapter, on CPU, with
  explicit English (German: automatic).
- Scoring: lower-cased words, punctuation removed. A 2,000-sample clip bootstrap
  gives the 95% interval.

## Results

### Held-out sets

| Set                                      | INT8 (previous) | 4-bit weight-only |  FP32 | 4-bit better / worse clips            | Median ASR, INT8 → 4-bit |
| ---------------------------------------- | --------------: | ----------------: | ----: | ------------------------------------- | ------------------------ |
| FLEURS en-US, 94 clips, 1,999 words      |           7.70% |         **5.95%** | 5.85% | 30 / 12; difference CI [−3.18, −0.49] | 217 → 335 ms             |
| FLEURS de-DE, 50 clips, 1,118 words      |           8.23% |         **6.62%** |     — | 19 / 10; difference CI [−3.32, 0.00]  | 294 → 451 ms             |
| FLEURS en-US, 16-clip screen pack        |           5.57% |             6.50% |     — | 3 more word errors                    | 208 → 299 ms             |
| Regression quiet lane (−30 dB), 24 clips |          72.70% |            68.75% |     — | —                                     | —                        |
| Synthetic controls, 16                   |          15.38% |      15.38–17.31% |     — | 1 word in two conditions              | —                        |
| Non-speech controls, 8                   |         0 false |           0 false |     — | —                                     | —                        |

The 94-clip set was prepared with `scripts/prepare-fleurs-benchmark.mjs`
(`--count 100`). The 6 sentences shared with the development clips were removed.
The German set holds the first 50 unique sentences of the pinned FLEURS `de_de`
test split. After repinning, the built-in path scored the same 5.95% on the
94 clips.

### Development clips (used to choose the precision)

72 clips from the [16 September regression corpus](audio-regression-2026-09-16.md)
(24 FLEURS sentences each clean, +10 dB and 0 dB noise; 1,824 words):

| Precision                        |  Clean | +10 dB |   0 dB |    All | Median ASR |
| -------------------------------- | -----: | -----: | -----: | -----: | ---------: |
| Dynamic INT8 (official)          | 15.13% | 14.47% | 25.16% | 18.26% |     261 ms |
| Dynamic INT8, per-channel signed | 13.49% | 16.45% | 30.76% | 20.23% |     334 ms |
| 4-bit weight-only                |  8.06% |  7.40% | 10.69% |  8.72% | 375–421 ms |
| 8-bit weight-only                |  7.07% |  7.24% | 10.53% |  8.28% |     718 ms |
| FP32                             |  7.07% |  7.24% | 10.53% |  8.28% |     466 ms |

An export of v3 made with the same NeMo 3.0 pipeline scored 17.98% in INT8, close
to the official 18.26%, so the export tooling does not explain the difference.

## Other Parakeet presets (not changed)

The same conversion was applied to the official FP32 exports of Parakeet TDT
0.6B v2 (`csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2@86891485`) and
Parakeet Unified English 0.6B
(`csukuangfj2/sherpa-onnx-nemo-parakeet-unified-en-0.6b-non-streaming@e69c9fb5`,
NVIDIA Open Model License). Both were compared with their current dynamic INT8
presets on the same 94 held-out FLEURS en-US clips:

| Preset           | Dynamic INT8 (shipped) | 4-bit weight-only | Better / worse clips | Difference CI | Median ASR   |
| ---------------- | ---------------------: | ----------------: | -------------------- | ------------- | ------------ |
| Parakeet v2      |                  4.95% |             5.05% | 10 / 12              | [−0.46, 0.67] | 196 → 279 ms |
| Parakeet Unified |                  5.45% |             5.75% | 11 / 13              | [−0.50, 1.15] | 271 → 397 ms |

Neither shows a difference within noise, and both are slower. Both presets keep
their INT8 exports, and no converted copies were published. The INT8 accuracy
loss is specific to the v3 export. FastConformer already ships FP32.

## Cohere Transcribe (not changed)

sherpa-onnx publishes Cohere only as dynamic INT8
(`csukuangfj2/sherpa-onnx-cohere-transcribe-14-lang-int8-2026-04-01@156a470c`), so
a full-precision copy was reconstructed locally for comparison:

1. The INT8 graph's dynamic-quantized operators (`MatMulInteger`,
   `ConvInteger`, `DynamicQuantizeLSTM`) were rewritten as FP32 operators with
   the stored 8-bit weights. This removes run-time activation quantization only.
2. Each dequantized weight was replaced by the original tensor from
   `CohereLabs/cohere-transcribe-03-2026@d552dadc`. Matching used shape and
   values: 450 encoder and 65 decoder tensors.
   - Median relative error of the INT8 weights against the originals: 1.9%; the
     worst layers reached 31–33%.
   - The next candidate was always at least 3× further away. Tied output and
     embedding weights count as one tensor.

Decoded with the pinned sherpa-onnx runtime (`11afbd0`) through a batch C-API
probe, CPU, 8 threads:

| Set                             | INT8 (shipped) | INT8 weights, no activation quantization | Original weights (FP32) | FP32 better / worse     | Median decode  |
| ------------------------------- | -------------: | ---------------------------------------: | ----------------------: | ----------------------- | -------------- |
| FLEURS en-US, 94 held-out clips |          5.85% |                                    5.75% |                   5.75% | 6 / 6; CI [−0.56, 0.30] | 460 → 917 ms   |
| FLEURS de-DE, 50 clips          |          4.74% |                                        — |                   4.56% | 6 / 4; CI [−0.97, 0.67] | 678 → 1,351 ms |

Despite the coarse per-tensor weight scales, Cohere's INT8 export shows no
accuracy loss outside noise, while full precision doubles decode time. Cohere is
unchanged. The reconstruction scripts and artifacts stayed local and nothing was
published.

## Limitations

- Only English and German read speech were measured. The other 23 supported
  languages, spontaneous dictation and long recordings were not.
- The quiet lane stays poor in both precisions.
- Memory, other CPUs and sustained thermals were not measured.
- The Cohere comparison used a batch C-API probe rather than the application's
  segmented adapter. It covers English and German read speech only.
- The Metal path (MLX FP32) is unaffected.
