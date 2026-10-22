# Apple Neural Engine spike for the Parakeet v3 encoder — 22 October 2026

Question ([#143](https://github.com/rahulsharmavishwakarma/vesper/issues/143),
owner-approved research spike): can Parakeet TDT 0.6B v3's encoder run on the
Apple Neural Engine (ANE) through Core ML with today's accuracy? What does it
change in latency, energy and contention with the Metal writing model?

**Result: yes, on this machine.** A Core ML fp16 encoder places 99.98% of its
estimated cost on the ANE. Through Vesper's own mel features and MLX TDT
decoder, it matches today's MLX path:

| Measure | Today's MLX path | ANE path |
| --- | --- | --- |
| WER, 94 held-out FLEURS en-US clips | 5.25% | 5.25% |
| WER, 100 unseen clips | 5.68% | 5.86% (within noise) |
| Per-clip time | 38–40 ms | 34–36 ms (about 10% faster) |
| Encoder energy per 15 s window | 961 mJ | 209 mJ (about 4.6× less) |
| Concurrent Metal writer's generation speed | −61% | −8% |

The spike changed no product code. Shipping it would add a new runtime path,
which needs its own proposal and approval.

All figures come from one Apple M5 Pro (macOS 27.0.1, build 26A434, Xcode
27.1). They are local measurements, not general claims. Older chips may place
operations differently and must be measured separately.

## Conversion

| Step | Detail |
| --- | --- |
| Source | `nvidia/parakeet-tdt-0.6b-v3@541d1f99`, `parakeet-tdt-0.6b-v3.nemo`, sha256 `3cbdc85877e668ca7b82d0d56770eb1fac76691f55d6b97545e8d61ca588d10d` (CC-BY-4.0) |
| Load | NeMo 2.3.1 `EncDecRNNTBPEModel.restore_from`, `.eval()` |
| Graph | An encoder-only wrapper: mel [1, 128, T] plus `mel_length` in; encoder [1, 1024, T′] plus length out. `torch.jit.trace` with torch 2.7.0 |
| Convert | coremltools 9.0, `mlprogram`, fixed shapes, `compute_precision=FLOAT16`, macOS 14 target |
| Windows | 15 s (1,501 mel frames → 188 encoder frames) and 30 s |
| Compression | fp16 (1.1 GB). Optionally 6-bit per-tensor k-means palettization (about 430 MB) |
| Compile | `xcrun coremlcompiler compile`, under 0.3 s |

The environment was a scratch uv project on Python 3.12. The Python 3.10 SciPy
wheels do not load on macOS 27.

**Tooling blocker.** coremltools 9.0 is tested up to torch 2.7, and torch
2.6/2.7 fail this repository's dependency audit (fixes are in torch 2.8–2.13).
A repository converter needs either coremltools on a non-vulnerable torch, with
encoder-output parity against this conversion, or an explicitly recorded
exception.

FluidInference's Apache-2.0 pipeline
([`mobius@26f5fa8`](https://github.com/FluidInference/mobius/tree/26f5fa8059d5ab3d34449adce6ff6ed86734e522/models/stt/parakeet-tdt-v3-0.6b/coreml))
and runtime
([`FluidAudio@c79c410`](https://github.com/FluidInference/FluidAudio/tree/c79c4106b954f81205113c3343e5a7e59a1089b5))
were used as references only. No code or weights from them were used.

## Placement

`MLComputePlan` (coremltools, `CPU_AND_NE`) puts 99.98% of the estimated cost
on the Neural Engine, for every window and compression variant. Four trivial
operations stay on the CPU: `cast` ×2, `expand_dims` and `less`, from the
length mask.

This is Core ML's plan, not a runtime trace. The ANE power rail during runs
(below) corroborates it.

## Encoder latency and accuracy

Measured with coremltools prediction, warm, median of 10. Cosine compares the
ANE output with NeMo fp32 PyTorch on the same mel.

| Window / variant | ANE | Core ML GPU | CPU | Cosine (ANE) |
| --- | --- | --- | --- | --- |
| 15 s fp16 | 24.3 ms | 14.4 ms | 82 ms | 0.9995 |
| 15 s 6-bit | 23.8 ms | 15.0 ms | 173 ms | 0.936 |
| 30 s fp16 | 81.7 ms | 27.4 ms | 170 ms | 0.995 |
| 30 s 6-bit | 72.6 ms | 26.1 ms | 306 ms | 0.895 |

The ANE wants windows of **at most 15 s**. At 30 s it is 3.4× slower and less
exact. Vesper's Parakeet parts are 30 s today.

WER isolates the encoder: NeMo frontend and NeMo greedy TDT decoder throughout.
Clips over 15 s (4 of 94) were split once at the quietest point.

| Encoder | WER (94 held-out) | Same text as fp32 | Δ vs fp32 (95% CI) |
| --- | --- | --- | --- |
| NeMo PyTorch fp32 | 5.30% | — | — |
| Core ML fp16, ANE | 5.25% | 87/94 | [−0.16, 0.00] |
| Core ML 6-bit, ANE | 5.40% | 61/94 | [−0.53, +0.72] |

## End to end through Vesper's Swift path

`native/mlx/Tests/VesperMLXTests/ParakeetNeuralEngineSpikeTests.swift` is
opt-in through `VESPER_ANE_SPIKE`. It runs:

1. Vesper's Parakeet mel;
2. the 15 s fp16 encoder with `computeUnits = .cpuAndNeuralEngine`;
3. the existing MLX TDT decoder (blocked loop), through test-only access.

It compares each clip with today's all-MLX path.

| Set | ANE path WER | MLX path WER | Δ (95% CI) | Same text | Median per clip, ANE vs MLX |
| --- | --- | --- | --- | --- | --- |
| FLEURS en-US held-out (94) | 5.25% | 5.25% | [−0.35, +0.34] | 79/94 | 34.4 vs 38.1 ms |
| FLEURS en-US unseen (100) | 5.86% | 5.68% | [−0.30, +0.65] | 85/100 | 35.9 vs 39.7 ms |

The encoder took a median 24.5 ms. Loading the cached model took 82–100 ms.

The ANE output buffer pads the time axis for alignment: 188 → 192, with
strides `[196608, 192, 1]`. Consumers must read through the strides; the
harness asserts this.

In Swift the encoder returns Float16. `MLShapedArray<Float16>` needs macOS 15,
so the harness reads the buffer directly.

## Load cost

| Encoder | First load (device specialization) | Cached load |
| --- | --- | --- |
| fp16, ANE | 7.7 s | 82–119 ms |
| 6-bit, ANE | 11.1 s | 54 ms |
| Core ML GPU | 0.8–3.9 s | about 0.6 s |

Core ML caches the specialization per model path and configuration. Apple
documents that a system update removes it
([WWDC23 10049](https://developer.apple.com/videos/play/wwdc2023/10049/)), so
the first-load cost recurs after each macOS update.

## Energy

`sudo powermetrics -i 200 -s cpu_power,gpu_power,ane_power`. Each lane ran its
15 s encoder window in a loop for 30 s, with idle baselines between lanes, on
AC power. Values are the mean over the last 25 s, above the idle baseline (CPU
1.26 W). powermetrics notes that its values are estimates.

| Encoder | CPU | GPU | ANE | Energy per 15 s window | Per second of audio |
| --- | --- | --- | --- | --- | --- |
| Core ML fp16, ANE | 2.10 W | 0.00 W | 7.84 W | 209 mJ | 14 mJ |
| Core ML fp16, GPU | 5.32 W | 32.27 W | 0 | 517 mJ | 34 mJ |
| MLX bf16, GPU (today's encoder) | 4.44 W | 28.19 W | 0 | 961 mJ | 64 mJ |

These are encoder-only figures. The TDT decoder stays on the GPU in every lane.

## Contention with the writing model

Setup:
- Quiet machine, AC power, two rounds in alternating order.
- Writer: llama.cpp at Vesper's pinned `d812350`, Metal, Qwen3.5-0.8B Q4_0,
  pp512/tg128. Solo: 248 tok/s generation, 9,879 tok/s prompt.
- Each ASR lane ran as a continuous background loop.

| Concurrent ASR | Writer generation | Writer prompt | ASR slowdown |
| --- | --- | --- | --- |
| Core ML fp16, ANE | −8% | −4% | ×1.32 |
| Core ML fp16, GPU | −48% | −29% | ×1.52 |
| MLX GPU (today) | −61% | −33% | ×1.40 |
| NVIDIA Q8_0 GGUF, NeMo-Speech.cpp Metal ([#293](https://github.com/rahulsharmavishwakarma/vesper/issues/293)) | −25% | −17% | ×2.84 |
| NVIDIA Q8_0 GGUF, NeMo-Speech.cpp CPU | −3% | −2% | ×1.96 |

Vesper's inference worker is serial today, so this applies to overlapping work:
- previews while polishing;
- other GPU apps;
- a future pipelined writer.

Comparison lane: NVIDIA's GGUF, `nemo-speech` 0.1.0 at `541d1f99`, on the 94
held-out clips.
- WER 5.05% on Metal and on CPU, against 5.25% for the MLX app path.
- About 59 ms mean per clip on Metal (`bench`, engine only), against 42.7 ms
  for MLX through the app.

## Limitations and risks

- **One machine.** Placement, latency and energy on M1–M4, and on lower-memory
  Macs, are unmeasured.
- **Window size.** At most 15 s per encoder window, which means a separate part
  length for this path.
- **First load.** 8–11 s, recurring after macOS updates.
- **Package size.** The fp16 encoder is 1.1 GB. The 6-bit encoder keeps WER
  within noise but changes more transcripts; it needs a broader quality check
  before it is preferred.
- **Languages.** Only English was measured. Parakeet's other 24 languages need
  their own lanes.
- **Tooling.** The conversion blocker above.
- **Precision.** The MLX decoder consumes fp16 encoder output, cast to bf16.
