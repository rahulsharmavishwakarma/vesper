# Parakeet v3 4-bit encoder: operator coverage and pointwise-convolution rewrite — 22 October 2026

Question ([#271](https://github.com/rahulsharmavishwakarma/vesper/issues/271)): why is the shipped Parakeet TDT 0.6B v3
4-bit weight-only encoder ([#251](https://github.com/rahulsharmavishwakarma/vesper/issues/251)) no smaller than the old
dynamic INT8 build, and can it be made smaller without losing accuracy or speed?

**Result (research evidence, not a qualification):**

- 303 MB of the 638 MB of encoder weights are pointwise (1x1) convolutions that ONNX Runtime's `MatMulNBits` quantizer
  leaves at FP32, because it only quantizes `MatMul`.
- Rewriting those convolutions as `Transpose -> MatMul -> Transpose` before quantizing cuts the encoder from 679.4 MB to
  **424.6 MB (−37%)**. The rewritten FP32 graph matches the original to 5e-7.
- On 100 FLEURS English clips through sherpa-onnx, the rewritten 4-bit encoder scored **5.59% WER** against 5.82% for FP32
  and 5.91% for the current app build. The difference is within noise (paired bootstrap 95% interval −0.87 to +0.46
  points against FP32).
- It ran at about the same speed as the current build on the tested CPUs.
- HQQ (data-free) made files larger and about 40-50% slower than round-to-nearest, with no accuracy gain.
- `accuracy_level=4` (int8 compute inside `MatMulNBits`) left WER unchanged. Its speed effect depended on the CPU: about
  1.75x faster on Apple M5 Pro, a few percent on AMD EPYC, none or slower on the Intel Xeon runtime.

Nothing in the application changed. Shipping the smaller encoder needs a pinned artifact and the held-out qualification
in [`benchmarks/asr/README.md`](../../benchmarks/asr/README.md).

## Method

Notebooks in [`notebooks/asr-quantization/`](../../notebooks/asr-quantization/README.md):

| Notebook | What it ran                                                                                                                                                                            |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 01       | Bytes by operator class (pointwise, depthwise and other convolutions split), encoder time by thread count, ONNX Runtime per-operator profile, `accuracy_level` set as a node attribute |
| 02       | Pointwise Conv-to-MatMul rewrite with an FP32 equivalence check; 4-bit block 32 variants with RTN (symmetric) and HQQ, each also with `accuracy_level=4`                               |
| 03       | sherpa-onnx greedy decoding of 100 FLEURS en-US test clips; lower-case, punctuation-free micro WER; 2,000-resample paired clip bootstrap against FP32                                  |

Sources: official FP32 export `csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3@1a468a35`, current app build
`rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4@8e178c8a`, FLEURS `google/fleurs@70bb2e84` (test
archive and metadata hashes checked; first 100 unique sentences of 4-20 s). Libraries: ONNX Runtime 1.31.0, ONNX 1.23.2,
sherpa-onnx 1.13.8, PyTorch 2.14.1+cpu (HQQ only), from the locked `benchmarks/asr/python/onnx-quantize` project.

Machines:

| Run                  | Machine                                                                                       |
| -------------------- | --------------------------------------------------------------------------------------------- |
| 01 local test        | Apple M5 Pro, 18 logical CPUs, macOS 27.0.1, Python 3.12.13                                   |
| 01                   | Colab CPU, Intel Xeon @ 2.20 GHz, 2 vCPUs, 12.7 GB, Python 3.13.16                            |
| 02 (first pass)      | Colab CPU, AMD EPYC 7B12, 2 vCPUs, 12.7 GB                                                    |
| 02 and 03 (reported) | Colab High-RAM CPU, AMD EPYC 7B12, 8 vCPUs, 51 GB, Python 3.13.16; sherpa-onnx with 4 threads |

Encoder timings in 01 and 02 use one random 10-second input (1,000 feature frames), 2 warm-ups and the median of 5 runs.
They measure speed only.

## Results

### Where the bytes are (identical on every machine)

| Encoder             | Total weights | `MatMul` / `MatMulNBits` | Pointwise Conv (50 nodes) | Depthwise Conv (26) |
| ------------------- | ------------: | -----------------------: | ------------------------: | ------------------: |
| FP32                |      2,435 MB |                 2,131 MB |                    303 MB |                1 MB |
| Current 4-bit build |        638 MB |                   333 MB |              303 MB (47%) |                1 MB |

### Where the time is

| Machine, threads      |     FP32 | Current 4-bit | `MatMulNBits` share of 4-bit time |
| --------------------- | -------: | ------------: | --------------------------------: |
| Apple M5 Pro, 4       |   110 ms |        385 ms |                               86% |
| Intel Xeon (Colab), 1 | 2,861 ms |      2,892 ms |                   79% (2 threads) |

`accuracy_level=4` on the same file (sizes 679.40 MB with or without it):

| Machine, threads      | Unset (current) | `accuracy_level=4` |
| --------------------- | --------------: | -----------------: |
| Apple M5 Pro, 8       |          224 ms |             128 ms |
| Intel Xeon (Colab), 2 |        2,857 ms |           2,921 ms |
| AMD EPYC (Colab), 1   |        2,138 ms |           2,030 ms |

### Variants (AMD EPYC, 8 vCPUs)

| Variant                            | Encoder size | Time, 1 thread | WER (2,166 words) | WER − FP32 [95% CI]  | Median decode per clip |
| ---------------------------------- | -----------: | -------------: | ----------------: | -------------------- | ---------------------: |
| FP32                               |     2,435 MB |              — |             5.82% | —                    |                 810 ms |
| Current app build                  |     679.4 MB |       2,033 ms |             5.91% | +0.09 [−0.50, +0.73] |                 741 ms |
| Original + RTN                     |     679.4 MB |       2,138 ms |             5.77% | −0.05 [−0.68, +0.55] |                 736 ms |
| Original + RTN, `accuracy_level=4` |     679.4 MB |       2,030 ms |             5.77% | −0.05 [−0.68, +0.55] |                 745 ms |
| Original + HQQ                     |     746.0 MB |       3,097 ms |             5.82% | 0.00 [−0.42, +0.37]  |               1,071 ms |
| Original + HQQ, `accuracy_level=4` |     746.0 MB |       3,074 ms |             5.82% | 0.00 [−0.42, +0.37]  |               1,055 ms |
| **Rewrite + RTN**                  | **424.6 MB** |       2,022 ms |         **5.59%** | −0.23 [−0.87, +0.46] |                 769 ms |
| Rewrite + RTN, `accuracy_level=4`  |     424.6 MB |       1,978 ms |             5.59% | −0.23 [−0.86, +0.45] |                 771 ms |
| Rewrite + HQQ                      |     500.7 MB |       3,253 ms |             5.77% | −0.05 [−0.53, +0.37] |               1,075 ms |
| Rewrite + HQQ, `accuracy_level=4`  |     500.7 MB |       3,200 ms |             5.77% | −0.05 [−0.53, +0.37] |               1,205 ms |

The rewrite converted 48 of the 50 pointwise convolutions; the other 2 did not meet its pattern (1-D kernel, stride 1, no
padding, group 1) and were left as they were. The 8-thread encoder timings are omitted: the same file measured 646 ms and
839 ms in one run, so differences there are noise on a shared machine.

### Published research build

The script now has `--rewrite-pointwise-conv`. Without it, it reproduced all four published files of the current app
build byte for byte; with it, two runs produced byte-identical files. The result, with the decoder and joiner quantized as
in the app build, is published for research as
[`rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4-pointwise@d2cd9553`](https://huggingface.co/rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4-pointwise/tree/d2cd9553882a740b9a21234c3daff4407e0a7821)
(encoder `a49c2769…`, 424,616,107 bytes). Uploaded file hashes were checked against the local build.

That exact artifact, scored with the same 100 clips on the Apple M5 Pro (sherpa-onnx 1.13.8 wheel, 4 threads):

| Build                   |   WER | Difference from FP32 [95% CI] | Median decode per clip |
| ----------------------- | ----: | ----------------------------- | ---------------------: |
| FP32 export             | 5.82% | —                             |                 149 ms |
| Current app build       | 5.91% | +0.09 [−0.50, +0.73]          |                 381 ms |
| Published rewrite build | 5.77% | −0.05 [−0.72, +0.65]          |                 435 ms |

On this Mac it decoded about 14% slower than the current build, while the Colab AMD runs showed about equal encoder time.

## Limitations

- 100 English read-speech clips only. No noise, quiet, German, Hindi, long-form, entity or number checks, which the
  shipped build was qualified on. A WER difference of 0.2 points is about 5 words here and is not significant.
- New variants keep the FP32 decoder and joiner, while the current app build also quantizes them. A shippable rewrite
  build would quantize them too and must be measured as such.
- Decoding used the sherpa-onnx pip wheel, not the application's sherpa-onnx build (ONNX Runtime 1.28.2) or `vesper-eval`.
  Whether the app's build loads the rewritten graph is untested.
- Colab CPUs are shared virtual machines. Their timings are not comparable with the desktop numbers in other records.
- Two Colab runtimes were recycled mid-run (about an hour after SSH-started jobs began). The reported 02 and 03 results
  come from one uninterrupted run on the High-RAM runtime.

## Next steps

1. Done: the rewrite is an explicit script option and the full artifact is published (above).
2. Qualify it through `vesper-eval` on the #251 held-out manifests (94 en-US, 50 de-DE, noise and quiet lanes) on the
   desktop, against the current build, including RAM and latency.
3. Check `accuracy_level=4` per platform before using it, since its speed effect depends on the CPU.
