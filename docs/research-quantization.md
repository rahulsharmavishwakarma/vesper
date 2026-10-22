# Quantization methods for local ASR and writing-model inference

Research page for [#265](https://github.com/rahulsharmavishwakarma/vesper/issues/265), dated 21 October 2026. It surveys quantization methods, what each needs, and what this repository's pinned runtimes can run. It builds on [research-streaming-inference-papers.md](research-streaming-inference-papers.md) and [inference-performance.md](inference-performance.md) and does not repeat them.

**Nothing here is implemented or approved.** No adapter, runtime, default or model entry changes. A quantized file is not application support: that needs a typed catalog entry, an architecture-specific adapter and pinned size/SHA-256/license manifests ([ADR-0003](adrs/0003-require-runtime-specific-model-adapters.md); see also [Hugging Face models](huggingface-models.md)).

## Evidence labels

| Label      | Meaning                                                                                                                              |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Paper/card | A published or vendor-reported result, not measured here. It does not transfer to another checkpoint, language, backend or hardware. |
| Runtime    | Verified in official source or documentation at the stated revision.                                                                 |
| Local      | Measured in this repository (existing reports only).                                                                                 |
| Unverified | Inference or memory, or seen only through an abstract or search summary. Treat as a hypothesis.                                      |

Three research passes read different sources, and the claims were not independently re-run. Paper figures marked as abstract-level need the full paper before they are quoted elsewhere.

## Method categories

Every method below falls into one of three categories from the project's research rules.

1. **Frozen-weight conversion.** Post-training quantization (PTQ) of the downloaded weights with public tools. Reachable by the application team today.
2. **Trained checkpoint.** Quantization-aware training (QAT), distillation or learned codebooks. Needs a training run and a new artifact; not a setting for downloaded weights.
3. **Runtime kernel work.** New data layouts or kernels in sherpa-onnx, ggml, MLX or a vendor runtime.

## What the pinned runtimes can load

| Runtime (pin)                           | Formats                                                                                                                    | Status                                                                                                                                              |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| llama.cpp `d8123504` (writer, LFM mtmd) | F32/F16/BF16, Q8_0, Q4_0, Q4_1, Q5_0, Q5_1; k-quants Q2_K to Q6_K; IQ1 to IQ4_XS                                           | Runtime. Sub-3-bit IQ types need an importance matrix.                                                                                              |
| same                                    | TQ1_0, TQ2_0 ternary                                                                                                       | Types and converter output exist (Runtime). Meant for ternary-trained models; poor for ordinary models (Unverified).                                |
| same                                    | MXFP4, NVFP4, Q1_0, Q2_0                                                                                                   | Runtime. MXFP4 has kernels on CPU, Metal, CUDA and Vulkan; NVFP4 has no Metal kernel ([backend coverage](#which-formats-run-on-every-backend)).     |
| same                                    | Online repacking of Q4_0, Q8_0, Q4_K and others on CPU                                                                     | Runtime. Automatic at load; not a file format.                                                                                                      |
| sherpa-onnx 1.13.8                      | FP32 and dynamic INT8 ONNX (CPU)                                                                                           | Runtime.                                                                                                                                            |
| sherpa-onnx 1.13.8                      | MatMulNBits int4                                                                                                           | The repository's weight-only int4 Parakeet v3 build loads (Local, [offline ASR](offline-asr.md)); behavior of other graphs unverified.              |
| NeMo-Speech.cpp 0.1.0                   | Converter: one type per file (q8_0, bf16, fp16, q6_k, q5_k, q4_k, nvfp4, mxfp4); loader: any stored type for linear layers | Runtime (`4f967622`). Depthwise and subsampling convs stay F16 and `pe` F32; see [the lowest-memory recipe](#lowest-memory-recipe-22-october-2026). |
| MLX (mlx-swift 0.31.4, mlx-audio-swift) | 4/8-bit affine groups, FP16/BF16                                                                                           | Only the exports pinned in `models/mlx-artifacts.json`; arbitrary exports need an adapter.                                                          |

The docs cite two llama.cpp revisions (`d8123504` current, `e79e4bf6` in older passages). Use `d8123504` and confirm against the `llama-cpp-sys-2` submodule when exactness matters.

Custom profiles accept only one sherpa-onnx NeMo transducer bundle with a single encoder, decoder, joiner and tokens file (no external tensor files), or one complete unsplit GGUF whose architecture and chat template the bundled llama.cpp supports ([Hugging Face models](huggingface-models.md)). ONNX int4 output written with external data is therefore rejected until merged into one file.

## Writing models (under 1B parameters)

| Method                                                         | Mechanism                                                                                   | Needs                                                      | Evidence for small models                                                                                                                                                                   | Reachable by                                                                          |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| GGUF Q8_0, Q4_0                                                | 32-weight blocks, fp16 scale; 8.5 and about 4.5 bits per weight                             | PTQ                                                        | Liquid LFM2.5-350M sizes: Q4_0 219 MB, Q8_0 379 MB, F16 711 MB (card)                                                                                                                       | Frozen weights                                                                        |
| GGUF k-quants (Q4_K_M, Q5_K_M, Q6_K)                           | 256-weight super-blocks with quantized sub-scales; "_M" mixes give some tensors more bits   | PTQ; importance matrix optional                            | LFM2.5-350M: Q4_K_M 229 MB, Q5_K_M 260 MB, Q6_K 293 MB (card)                                                                                                                               | Frozen weights                                                                        |
| i-quants (IQ1 to IQ4_XS)                                       | Codebook-style vector quantization                                                          | PTQ; importance matrix strongly advised below about 3 bits | None found below 1B; very low bit widths likely unusable at this size (Unverified)                                                                                                          | Frozen weights                                                                        |
| Importance matrix and per-tensor overrides                     | Activation statistics weight the rounding error; flags keep embedding/output tensors higher | Calibration text                                           | Unsloth notes wiki-style calibration can overfit and text-only calibration suits instruct models poorly (vendor page)                                                                       | Frozen weights                                                                        |
| GPTQ, AWQ, HQQ, AutoRound                                      | Second-order or activation-aware rounding; weight-only                                      | Calibration data (HQQ none)                                | The Qwen3 quantization study ([arXiv 2505.02214](https://arxiv.org/abs/2505.02214)) reports degradation at ultra-low precision; per-size 0.6B numbers not extracted                         | Frozen weights, but not loadable by llama.cpp; MLX has `mlx_lm.gptq` and `mlx_lm.awq` |
| MLX dynamic quant, DWQ                                         | Mixed 4/5-bit; DWQ tunes scales against the unquantized teacher                             | Calibration; DWQ a short training run                      | No numbers on the source page                                                                                                                                                               | Apple silicon only                                                                    |
| Quantization-aware distillation (Liquid QAD)                   | Q4_0 checkpoints trained by distillation; FP32 source in the repo's `qad/` folder           | Training                                                   | Vendor claims about 97% of BF16 average score and quality comparable to Q5_K_M at 230M/350M; secondary summaries add decode gains of 4% to 33%. Vendor-reported, not independently verified | Trained checkpoint (published for LFM2.5)                                             |
| ParetoQ ([arXiv 2502.02631](https://arxiv.org/pdf/2502.02631)) | QAT across 1, 1.58, 2, 3 and 4 bits                                                         | Training                                                   | Reports ternary, 2-bit and 3-bit trading off better than 4-bit and binary; general benchmarks, not cleanup                                                                                  | Trained checkpoint; no llama.cpp path found                                           |
| BitNet b1.58 / bitnet.cpp                                      | Ternary weights with I2_S, TL1, TL2 kernels                                                 | Trained model                                              | Reported CPU speedups of 1.37x to 5.07x (ARM) and 2.37x to 6.17x (x86) against its baselines; Metal not stated                                                                              | Runtime kernel work and a trained model                                               |
| KV-cache quantization                                          | Quantized keys and values                                                                   | PTQ                                                        | Not measured for small models; short polishing prompts make the cache small next to the weights (Unverified)                                                                                | llama.cpp cache types                                                                 |

**Tied embeddings.** In a 230M to 0.8B model the vocabulary matrix is a large share of parameters, and the output head shares it. Keeping it at Q6_K or Q8_0 while the blocks go lower costs size but protects output logits (Unverified; supported by llama.cpp's `--token-embedding-type` and `--output-tensor-type` options).

**Plain Q4 at this scale.** Liquid's QAD result suggests plain Q4 PTQ loses measurable quality below about 1B parameters, which QAD tries to recover. This is an inference from a vendor claim, not a measurement here.

## ASR models

| Method                                        | Mechanism                                                                        | Needs                                              | Evidence                                                                                                                                                                                                                                                                                                                                          | Reachable by                              |
| --------------------------------------------- | -------------------------------------------------------------------------------- | -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| ONNX dynamic INT8                             | Activation scales at run time                                                    | PTQ                                                | Local: Parakeet v3 FLEURS English 7.70% WER vs 5.95% for weight-only int4, German 8.23% vs 6.62% ([offline ASR](offline-asr.md))                                                                                                                                                                                                                  | Frozen weights                            |
| ONNX static INT8 (QDQ)                        | Offline activation scales                                                        | PTQ with calibration                               | No Parakeet number; ORT suggests static for CNNs and dynamic for transformers/RNNs                                                                                                                                                                                                                                                                | Frozen weights                            |
| ONNX weight-only int4 (MatMulNBits)           | Block-wise int4 weights, float activations                                       | RTN, HQQ or GPTQ                                   | Local: Parakeet v3 block 32 gave 5.95% English and 6.62% German WER (INT8: 7.70% and 8.23%), but the download size was about the same and CPU decoding roughly 1.5x slower ([offline ASR](offline-asr.md), [precision record](validation/parakeet-weight-only-2026-10-21.md)). Conv layers stay FP32 because only MatMul and Gather are quantized | Frozen weights                            |
| ggml k-quants for ASR                         | Block quantization of 2D weights                                                 | PTQ                                                | Paper: [arXiv 2604.14493](https://arxiv.org/abs/2604.14493) reports int4 within 1 point WER for one Nemotron pipeline (2.47 GB to 0.67 GB) with attention projections and first/last encoder layers kept at int8. Not transferable                                                                                                                | Frozen weights                            |
| whisper.cpp quantization                      | Quantizes all tensors except a skip list (conv biases and positional embeddings) | PTQ                                                | Paper: [arXiv 2503.09905](https://arxiv.org/abs/2503.09905) reports int4/int5/int8 with accuracy preserved (abstract-level)                                                                                                                                                                                                                       | Frozen weights                            |
| 4-bit Conformer QAT                           | Integer QAT                                                                      | Training                                           | Paper: [arXiv 2203.15952](https://arxiv.org/abs/2203.15952) reports lossless 4-bit on LibriSpeech; production-scale models needed regression fixes                                                                                                                                                                                                | Trained checkpoint                        |
| General Quantizer                             | Learned centroids                                                                | Training                                           | Paper: [arXiv 2210.09188](https://arxiv.org/abs/2210.09188) reports sub-8-bit RNN-T and Conformer without loss                                                                                                                                                                                                                                    | Trained checkpoint                        |
| Whisper 4-bit PTQ                             | NF4/FP4/INT4                                                                     | PTQ                                                | Interspeech 2026 study of tiny.en and base.en: activation bit width mattered more than weight format; abstract-level                                                                                                                                                                                                                              | Frozen weights                            |
| Core ML palettization, MLX group quantization | LUT clustering; 2 to 8-bit groups                                                | PTQ                                                | No ASR numbers fetched. Local: in the matched Qwen screen, MLX 8-bit worsened quiet-condition WER from 9.58% (Sherpa INT8) to 16.26% and noisy WER from 13.35% to 20.17% (see [inference performance](inference-performance.md))                                                                                                                  | Apple platforms only                      |
| parakeet-redux (ternary encoder)              | Encoder weights in {-1, 0, +1}                                                   | Trained (method not stated)                        | Card: Open ASR average 6.55% vs 6.26% original; FLEURS 10.56% vs 11.62%; noise 9.04% vs 6.72%                                                                                                                                                                                                                                                     | Vendor runtime (Photon)                   |
| Phonon-2 (about 2.1 bits)                     | Five learned levels in the encoder                                               | Trained; card tag says quantization-aware training | Card: English average 5.21% vs 4.96% teacher                                                                                                                                                                                                                                                                                                      | Vendor runtimes (MLX, Core ML, CPU, CUDA) |

**Layer sensitivity (Unverified, from general practice and ORT docs).** Linear layers tolerate int4 with block scaling. Depthwise convolutions, layer norms, the subsampling front end, the transducer decoder and joint, and CTC output projections should stay at higher precision unless a WER gate passes. Activation quantization is where attention outliers hurt, which is consistent with the local dynamic INT8 result but does not prove its cause.

**Both extreme low-bit cards omit** the training method, data, decoder/joint precision and an ONNX or GGUF artifact. Neither can run in sherpa-onnx or ggml without the vendors' weight format and kernels. Neither states Hindi results.

## Why quantization speeds up inference

Everything is matrix multiplication with dequantization fused into the kernel. ggml splits each weight row into fixed blocks with their own scales, quantizes activations on the fly to Q8, and runs integer dot products on NEON dotprod/i8mm or AVX2/AVX512-VNNI. Weights are never expanded to float in memory. Online repacking interleaves rows for wide SIMD at load.

- Token-by-token decoding is memory-bandwidth bound, so fewer bits read per token help. Prefill (the polishing prompt) is compute-bound and gains little.
- It stops being "just matmul" for norm weights, biases, small depthwise convolutions, recurrent or state-space parameters (LFM2 short-conv, Granite hybrid, Qwen3.5 gated-delta) and any 1-D tensor. These stay F32/F16. ggml quantizes only 2-D tensors whose row length divides the block size (32 for Q4_0/Q8_0, 256 for k-quants) and falls back for others.
- No speedup appears when a tiny model is dominated by launch or sampling overhead, when the kernel is immature for the format (i-quants, FP4 without native support), or when prompts are long relative to output (Unverified).
- A smaller file does not guarantee lower latency or RAM; measure ([inference performance](inference-performance.md)).

## Conversion tooling

Command shapes were composed from documentation and source reading. Nothing was run; verify with each tool's `--help` before use.

```sh
# llama.cpp (LLMs)
python convert_hf_to_gguf.py HF_DIR --outfile m-f16.gguf --outtype f16
llama-imatrix -m m-f16.gguf -f calib.txt -o imatrix.gguf --parse-special
llama-quantize --imatrix imatrix.gguf m-f16.gguf m-q4_k_m.gguf Q4_K_M
llama-quantize --token-embedding-type q8_0 --output-tensor-type q8_0 m-f16.gguf m-q4_0.gguf Q4_0

# NeMo-Speech.cpp
python convert_model.py nvidia/REPO --outfile asr.gguf --outtype q8_0

# ONNX Runtime weight-only int4: the 4-bit MatMul quantizer in onnxruntime.quantization (RTN, HQQ or GPTQ; opset 21)
```

Converter coverage at llama.cpp `d8123504`: LFM2 text, Granite dense and hybrid (`GraniteMoeHybridForCausalLM`), Qwen3 and Qwen3.5. Moonshine has no converter entry there. Parakeet and FastConformer exist only as multimodal audio encoders, not standalone decoders; use NeMo-Speech.cpp for Parakeet and Nemotron GGUF.

## Calibration data and licensing

- Use public or synthetic calibration text and audio only. This project forbids private audio and transcripts, and imatrix, GPTQ and AWQ statistics can encode properties of the calibration set. Record the corpus, hash and license in the artifact manifest.
- A re-quantized model is a derivative. Keep upstream notices and record the base revision. CC-BY-4.0 (Parakeet derivatives) requires attribution and a change statement; Apache-2.0 (Granite, Qwen) requires the license text and notices. Read the LFM Open License 1.0 and NVIDIA Open Model License terms on derivatives before redistributing; they were not read in full here. This is not legal advice.

## Proposed verification protocol (not an existing procedure)

1. Freeze the envelope: application, runtime and converter revisions, artifact hashes, calibration corpus hash, commands, hardware, backend, threads, resident or cold state.
2. Compare each quantized artifact with the highest-fidelity artifact the runtime can run (F16/BF16 GGUF, FP32 ONNX), using identical inputs.
3. Writers: perplexity and KL divergence on held-out public text, then the real cleanup prompts on a synthetic dictation set, checking numbers, entities, negations, added content and guard trigger rates. Preserve failures in aggregates.
4. ASR: WER/CER on the held-out manifests by language and clean versus noise-mixed, plus entity and number accuracy, a noise sweep and long-form repetition checks. Compare with the current INT8 as well as the reference. Treat differences on small corpora as noisy.
5. Resources: peak and steady RAM/VRAM, load time, first partial, stop-to-final, tokens per second, sustained thermal behavior, with alternating A/B runs.
6. Acceptance: no regression beyond a margin fixed before the run, pinned manifests, adapter tests and a license record. Until then the artifact stays a candidate.

## Reachable by category

| Category                 | Writing models                                                                                                                 | ASR models                                                                                                                                                                              |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frozen-weight conversion | GGUF Q8_0 through Q4_K_M with a task-specific importance matrix and per-tensor overrides; MLX affine 4/6/8-bit                 | ONNX weight-only int4 (extend the existing script), ggml k-quants with F16 fallback, MLX/Core ML on Apple platforms. About 4 bits per weight looks reachable; ternary and 2-bit are not |
| Trained checkpoint       | Liquid QAD (published for LFM2.5), ParetoQ-style 2 to 3 bit, BitNet; a QAT or QAD pass after fine-tuning to recover Q4 quality | Ternary and about 2-bit encoders, lossless 4-bit Conformer QAT, static INT8 that beats weight-only                                                                                      |
| Runtime kernel work      | BitNet kernels, MXFP4/NVFP4 on CPU or Apple GPU, W4A4 or SmoothQuant paths, sub-3-bit i-quant speed on Metal                   | Ternary or five-level kernels, faster CPU int4, int4 for conv layers                                                                                                                    |

## Recent ISTA-DASLab work (read 21 October 2026)

The [IST-DASLab GitHub organization](https://github.com/orgs/IST-DASLab/repositories) (the group behind GPTQ) pushed several quantization repositories in 2026. This table comes from each repository's README and the paper links it gives. The papers themselves were not read, and none of the results were reproduced or run here. Most target CUDA and vLLM, not CPU or Metal.

| Repository                                                                                                                                    | Idea                                                                                                                              | Needs                                                                                                           | Relevance to this project                                                                                                                                                                                                                                                              |
| --------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [disaggregated-quantization](https://github.com/IST-DASLab/disaggregated-quantization) ([arXiv 2609.26333](https://arxiv.org/abs/2609.26333)) | Different number formats for prefill and decode: compact weights for decode, native NVFP4 for prefill                             | Quantization-aware distillation for full results; the shared-weight variant on large models needs no retraining | Repo tests Qwen 3 0.6B to 8B and Gemma 3 270M to 12B and reports gains at 2-bit decode against trained weight-only baselines. Prefill is a small part of a short polishing request, and the page names no CPU or Metal support, so it is a research reference only. License not stated |
| [GSQ](https://github.com/IST-DASLab/GSQ) ([arXiv 2604.18556](https://arxiv.org/abs/2604.18556))                                               | Learns per-weight choices on a small grid with a Gumbel-Softmax relaxation, starting from GPTQ; supports 2-bit, n-bit and ternary | Post-training optimization on NVIDIA GPUs                                                                       | Can refine existing GGUF k-quant checkpoints and save them in the same format, which is the one path found that could reach llama.cpp. README reports Qwen3-8B GGUF Q2_K average rising from 50.03 to 56.28. Only a Qwen3-0.6B smoke test is mentioned for small models. Apache-2.0    |
| [SLQ](https://github.com/IST-DASLab/SLQ) ([arXiv 2605.02404](https://arxiv.org/abs/2605.02404))                                               | Searches per-layer bit widths from 2 to 8 for the lowest size that meets a fidelity target                                        | Post-training, FineWeb-Edu calibration tokens, CUDA                                                             | A principled way to choose per-layer bits, but output is fake-quantized BF16 for vLLM with Humming kernels, not GGUF. Apache-2.0                                                                                                                                                       |
| [MatGPTQ](https://github.com/IST-DASLab/MatGPTQ) ([arXiv 2602.03537](https://arxiv.org/abs/2602.03537))                                       | One parent model sliced to 2, 3, 4, 6 or 8 bits                                                                                   | Post-training                                                                                                   | Would let one download serve several device classes, but kernels are vLLM and CUDA, and no small-model results are shown. MIT                                                                                                                                                          |
| [EvoPress](https://github.com/IST-DASLab/EvoPress) ([arXiv 2410.14649](https://arxiv.org/abs/2410.14649))                                     | Evolutionary search for per-layer bit widths, sparsity or depth pruning                                                           | Post-training                                                                                                   | Per-layer allocation idea; no GGUF export mentioned. Apache-2.0                                                                                                                                                                                                                        |
| [disaggregated-llama.cpp](https://github.com/IST-DASLab/disaggregated-llama.cpp)                                                              | A llama.cpp fork                                                                                                                  | Not stated                                                                                                      | The README shown is upstream's, so what the fork changes could not be determined from it. Its base revision is unknown                                                                                                                                                                 |

None of these is an application setting, and none changes the findings above: CPU and Apple-silicon paths depend on formats that ggml, ONNX Runtime or MLX can execute. The concrete idea worth testing is whether a GSQ-style refinement of a small model's GGUF k-quant beats the plain k-quant on the polishing benchmark (an open question below).

## Measured since this survey

Later work measured several of the open questions below on this repository's models. These are local results with the
limits stated in each record.

- **ONNX operator coverage.** In the Parakeet v3 4-bit encoder, 303 MB of pointwise convolutions stayed FP32 because
  `MatMulNBits` only covers `MatMul`. Rewriting them as `MatMul` first cut the encoder from 679 MB to 425 MB with WER
  unchanged within noise; HQQ was larger and slower than round-to-nearest
  ([record](validation/parakeet-conv-rewrite-2026-10-22.md), #271).
- **MLX, Cohere and one-file GGUF.** 8-bit MLX quantization of Parakeet's `Linear` layers, a pointwise rewrite for
  Cohere Transcribe's int8 export, and NVIDIA's Q8_0 GGUF through NeMo-Speech.cpp (one file for CPU and GPU, with
  K-quants losing accuracy) are recorded under #287. Offering that GGUF in the app is #293.
- **Calibrated or learned rounding** (GPTQ, GSQ, DWQ) and quantization-aware training remain untested here.

## Lowest-memory recipe (22 October 2026)

Research for [#316](https://github.com/rahulsharmavishwakarma/vesper/issues/316). It covers the files that Automatic actually loads ([ADR-0026](adrs/0026-automatic-speech-engine.md)):

- the Parakeet v3 and Nemotron GGUFs through NeMo-Speech.cpp `4f967622` (ggml `c03b4e2b`);
- the writing model through llama.cpp `d8123504`;
- the Core ML Parakeet encoder on the Neural Engine ([ADR-0025](adrs/0025-neural-engine-speech-engine.md)).

Pinned source and the shipped files were read directly. **Nothing here is implemented.** A new converter or a smaller shipped file still needs its own proposal, approval and model qualification.

### Where the bytes are

| File                                             |              Size | Largest parts                                                                                                                                                        |
| ------------------------------------------------ | ----------------: | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Parakeet v3 `q8_0.gguf` (`541d1f99`)             |            714 MB | Encoder linears at Q8_0: FFN 428 MB, attention 134 MB, pointwise convs 80 MB. F32 `encoder.pos_enc.pe` table: 41 MB. Decoder, joint, LSTM and embedding: about 24 MB |
| Writing model `Qwen3.5-0.8B-Q4_0.gguf`           |            563 MB | `token_embd` at **Q8_0, 270 MB**, tied as the output head. The other blocks at Q4_0 total about 280 MB                                                               |
| Neural Engine encoder (int8 linear, per channel) | 594 MB of weights | All encoder weights; float16 is 1,186 MB                                                                                                                             |

All three rows are Local: gguf-py inspection of the published Parakeet file and the installed writer file, and the [Neural Engine record](validation/parakeet-neural-engine-2026-10-22.md). Nemotron has the same encoder, a larger joint output and 17.8 MB of F32 `prompt_kernel` tensors.

### Which formats run on every backend

Runtime evidence, from `supports_op` and the kernel tables at both pins:

| Format       | Bits per weight | CPU / Metal / CUDA / Vulkan | Notes                                                   |
| ------------ | --------------: | --------------------------- | ------------------------------------------------------- |
| Q8_0         |             8.5 | ✓ ✓ ✓ ✓                     | Current speech format                                   |
| Q6_K         |            6.56 | ✓ ✓ ✓ ✓                     | Rows must be a multiple of 256 wide                     |
| Q5_0         |             5.5 | ✓ ✓ ✓ ✓                     | 32-wide blocks                                          |
| Q5_K         |             5.5 | ✓ ✓ ✓ ✓                     | Rows must be a multiple of 256 wide                     |
| Q4_0, IQ4_NL |             4.5 | ✓ ✓ ✓ ✓                     | 32-wide blocks; have Metal small-batch kernels          |
| Q4_K         |             4.5 | ✓ ✓ ✓ ✓                     | Rows must be a multiple of 256 wide; generic Metal path |
| MXFP4        |            4.25 | ✓ ✓ ✓ ✓                     | 32-wide blocks                                          |
| IQ4_XS       |            4.25 | ✓ ✓ ✓ ✓                     | Rows must be a multiple of 256 wide                     |
| NVFP4        |             4.5 | ✓ ✗ ✓ ✓                     | No Metal kernel; excluded                               |
| TQ1_0, TQ2_0 |         1.7–2.1 | ✓ ✗ ✗ ✗ at ggml `c03b4e2b`  | Metal aborts at run time; excluded                      |

Formats below about 4 bits per weight also have kernels everywhere, but the ASR evidence below argues against them.

Parakeet's LSTM, `joint.pred` and `joint_net` tensors are 640 wide, so only the 32-wide-block formats fit them. When NeMo's converter makes a K-quant file, it stores those tensors as F16, so the local plain Q6_K/Q5_K/Q4_K losses (#287) came from the encoder.

Runtime constraints at `4f967622`:

- Depthwise and subsampling conv kernels must be F16, and `pe` must be F32.
- Within a layer, q, k and v must share one type, because the fused loader rejects mixed types.
- Linear and pointwise layers accept any stored type, so a mixed-precision file loads without runtime changes.

### Memory behaviour

Runtime evidence:

- **NeMo-Speech.cpp** reads each tensor into backend buffers with `fread` rather than mapping the file, and it doesn't use CPU repacking. A smaller file therefore lowers resident memory by about the same amount.
- **Activations:** for dictation-length audio the weights dominate. Attention memory grows with the square of the frame count, so long recordings are bounded by length, not by weight precision.
- **llama.cpp** maps the GGUF into memory. On Metal it wraps the mapped pages without copying them.
- **CPU repacking** of Q4_0, Q8_0 and K-quants writes a second, repacked copy into anonymous memory, so a CPU-only writer run holds more than the file size.

### Recommendation

**Speech GGUF (Parakeet, Nemotron): our own mixed-precision file in standard formats.**

1. Write a converter wrapper in the locked `benchmarks/asr/python/nemo-convert` project.
   - It overrides the converter's per-tensor type choice (`conversion.asr._pick_dtype`) with a regex-to-type table and quantizes from the original `.nemo` checkpoint, never from Q8_0.
   - The output is an ordinary GGUF that the pinned runtime already loads on every backend. This is a frozen-weight conversion; no runtime change is needed.
2. Add a speech importance matrix.
   - Collect the mean squared input per column for each linear layer with PyTorch hooks over public multilingual audio, using valid frames only.
   - Pass it to `ggml_quantize_chunk`, which already accepts one. The converter currently passes `None`.
   - This affects only tensors below Q8_0. Whether it recovers the plain K-quant loss is Unverified.
3. Sweep these candidate mixes (sizes computed from the tensor list; Unverified accuracy):

   | Mix  | Recipe                                                                                                            |   File |
   | ---- | ----------------------------------------------------------------------------------------------------------------- | -----: |
   | Q8_0 | Published                                                                                                         | 714 MB |
   | A    | Layers 0, 1, 22, 23 at Q8_0; elsewhere attention and FFN linear2 at Q6_K, FFN linear1 and pointwise convs at Q5_K | 561 MB |
   | B    | Attention at Q8_0 in every layer; layers 0 and 23 fully Q8_0; FFN and pointwise at 4.5 bits                       | 495 MB |
   | C    | Same size as B, but protects FFN-down instead of attention (llama.cpp's Q4_K_M rule)                              | 495 MB |
   | D    | Layers 0 and 23 at Q8_0; the rest of the encoder at 4.5 bits                                                      | 437 MB |

   In every mix, the decoder, joint and LSTM stay at Q8_0, depthwise and subsampling convs at F16, and norms, biases and `pe` at F32.

   For mixes B–D, compare Q4_K against Q4_0/IQ4_NL at the same 4.5 bits per weight. The 32-wide formats have specialised Metal small-batch kernels.

   Mix B follows the closest published result, Paper [arXiv 2604.14493](https://arxiv.org/abs/2604.14493): Nemotron int4 with attention and the first and last layers kept at int8 lost 0.09 average WER points (0.73 GB). It differs here in format and checkpoint.

   Other evidence:
   - Plain 4-bit post-training quantization of a Conformer lost 0.1–0.4 points (Paper, [GenPTQ](https://aclanthology.org/2025.findings-emnlp.566/)).
   - Quantization-aware-training studies point the same way: attention and upper layers are the sensitive parts (Paper, [arXiv 2203.15952](https://arxiv.org/abs/2203.15952)).

4. Expect smaller memory, not more speed. The encoder's large matrix multiplies are compute-bound, and the local K-quants were not faster.

Generating `pe` at load would save another 41 MB, but it patches vendored runtime code and needs separate approval.

**Writing model (llama.cpp): fix the embedding first, then calibrate.**

1. Re-quantize the tied embedding from Q8_0 to Q6_K with `--token-embedding-type`. That saves about 62 MB, or about 127 MB at Q4_0. Its quality is untested.
2. Use an importance-matrix Q4_0 at the same size. It's the standard format, built with `llama-imatrix` and `llama-quantize --imatrix`.
3. Run a per-tensor sweep under a size budget with `--tensor-type-file`, scored by KL divergence against BF16 and by the polishing benchmark. The Q4_0 vs Q8_0 question from #308 belongs in this sweep.

**Neural Engine (Core ML encoder):** measured results and the remaining candidates are in [Neural Engine options](#neural-engine-options).

**Not recommended now:**

| Method                        | Reason                                                                                                               |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| GSQ                           | No public GGUF reader or writer                                                                                      |
| Unsloth Dynamic 2.0           | Selection procedure and calibration data unpublished                                                                 |
| Rotations (QuaRot, SpinQuant) | Need online Hadamard ops or untying the embedding; SpinQuant is non-commercial                                       |
| IST-DASLab gptq-gguf-toolkit  | No license                                                                                                           |
| AutoRound                     | Possible for the writer on CUDA; no NeMo path                                                                        |
| NVFP4, TQ formats             | Backend gaps above                                                                                                   |
| Below about 4 bits for speech | Per-channel 4-bit post-training quantization already fails badly on Conformers (Paper, arXiv 2203.15952, 2312.08553) |

### Proposed qualification plan

1. **Sensitivity scan.** Quantize one tensor group at a time against an F16 reference. Score with encoder-output cosine, per-frame KL divergence of the token and duration heads, and the identical-transcript rate.
2. **WER lanes:**
   - held-out FLEURS for the priority languages, at least 20,000 words per lane;
   - the quiet and noisy regression lanes;
   - 60–300 s long-form audio.
3. **Statistics:** paired bootstrap against Q8_0, with equivalence pre-registered at +0.2 points pooled and +0.5 per language.
4. **Memory and speed:** peak resident memory and physical footprint, load time, and CPU and Metal speed on the same build. Add CUDA and Vulkan when hardware is available.
5. **Calibration audio:** public only (FLEURS train/dev, Common Voice, VoxPopuli, MLS), balanced per language, never the reported test split. Record hashes and licenses with the artifact.
6. **Decision rule:** keep the smallest mix that passes every lane. Changing the shipped file or a default still follows [model qualification](../benchmarks/asr/README.md).

### Neural Engine options

Measured on the Parakeet v3 encoder by #143 (Local, [Neural Engine record](validation/parakeet-neural-engine-2026-10-22.md)). Setup:

- macOS 14 target, coremltools 9.0, Apple M5 Pro.
- 94 held-out clips, scored with the Swift parity harness and paired against float16.
- Every variant kept 99.92% of estimated cost on the Neural Engine at about 23–25 ms per 15 s window.

| Variant                                |  Weights |   WER | Δ vs float16 [95% CI]           |
| -------------------------------------- | -------: | ----: | ------------------------------- |
| float16                                | 1,186 MB | 5.34% | —                               |
| 8-bit palettized, per tensor           |   593 MB | 5.44% | +0.10 [0.00, +0.26]             |
| **int8 linear, per channel (shipped)** |   594 MB | 5.24% | −0.10 [−0.25, 0.00]             |
| 6-bit palettized, per tensor           |   445 MB | 5.59% | +0.25 [−0.31, +0.91] (rejected) |

**Placement of the other coremltools 9.0 modes.** Local, M5 Pro, macOS 27.0.1, `.cpuAndNeuralEngine` (the app's setting). Checked with `MLComputePlan` on a synthetic stack of FastConformer-sized linears, so it shows placement only, not WER:

| Stays on the Neural Engine                                                                                                                                         | Falls back to CPU (4–14× slower)                                                                          |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| int8 and int4 per channel; 6-bit and 4-bit per-grouped-channel palettization (group 16); 4-bit palettization with an int8 lookup table; 4-bit vector palettization | **Block-wise int4** (block 32 and 128); palettization with per-channel scales; 8-bit vector palettization |

What this means:

- Block-wise int4 is the Core ML mode closest to GGUF Q4_0, and it is the one to avoid on the Neural Engine. Apple describes it as GPU-oriented, and one third-party report also saw it fall back (Unverified, [ANE book](https://alvaro-videla.com/ane-book/03-quantization.html)).
- Under `.all`, Core ML put the whole encoder on the GPU, so the app must keep `.cpuAndNeuralEngine`.
- While each encoder ran, system wired memory rose by roughly its compressed size: int8 about 610–690 MB, 6-bit about 475 MB, int4 per channel about 335 MB (Local, noisy system counter). This is consistent with Apple's statement that the Neural Engine decompresses weights just in time from macOS 14 (Runtime, [WWDC23 10047](https://developer.apple.com/videos/play/wwdc2023/10047/)).

**Other ports ship smaller encoders** (Unverified for this app):

- FluidInference and Argmax both ship 6-bit per-tensor palettized Parakeet v3 encoders (445 and 458 MB).
- FluidInference also publishes an int4 per-channel encoder (298 MB) with no published WER.

The per-tensor 6-bit result above failed this project's gate, so their choice doesn't transfer.

**Recommended next steps, in order:**

1. **6-bit per-grouped-channel palettization, group 16 (about 450 MB).** It stays on the Neural Engine and needs a macOS 15 target. If data-free k-means misses the gate, use calibrated sensitive k-means (`SKMPalettizer`).
2. **Mixed-bit palettization (about 350–420 MB).** Assign 4, 6 or 8 bits per layer from an encoder-output sensitivity analysis on public calibration audio, using only modes from the "stays" column. Apple's mixed-bit pre-analysis in [ml-stable-diffusion](https://github.com/apple/ml-stable-diffusion) (MIT) is a usable starting point.
3. **int4 per channel or 4-bit grouped palettization (about 300 MB).** Only with GPTQ or sensitive-k-means calibration: a data-free 4-bit build of another model lost badly (Paper/card, FluidAudio's SenseVoice result).

Each step must pass the held-out WER gate and pass `MLComputePlan` placement on more than one chip before shipping.

Excluded:

- **W8A8 activation quantization:** no download or memory saving over int8 weights. Apple's faster int8 compute applies only from A17 Pro/M4.
- **Pruning without fine-tuning.**
- **OD-MBP outlier decomposition** (WhisperKit, [arXiv 2507.10860](https://arxiv.org/abs/2507.10860)): not available in open tooling.

#304 is measuring Neural Engine placement and WER for two exact-format encodings on smaller encoders. The block-wise one may hit the CPU fallback above.

The Core ML encoder and the GGUF cannot share weight files. Each runtime needs its own container, and the closest shared numeric format (block-wise int4) is the one that leaves the Neural Engine.

## Open questions and candidate experiments (proposed)

- Does the weight-only int4 recipe generalize to Granite Speech 5.0 TurboCTC and Moonshine? No quantization data was found for either.
- How much does plain Q4_K_M lose against Q8_0 on the polishing task at 230M to 0.8B, and does a task-specific importance matrix close the gap?
- Can the Liquid QAD recipe be reproduced from the FP32 `qad/` weights, and does LoRA fine-tuning on a quantized base preserve quality? The second question is unverified.
- Does sherpa-onnx 1.13.8 load MatMulNBits or FP16 transducers other than the existing Parakeet build?
- Does a GSQ-style refinement of a Qwen3-0.6B or LFM2.5 GGUF k-quant beat the plain k-quant on the polishing benchmark? Its README names only a Qwen3-0.6B smoke test for small models.
- Can ParetoQ-style training produce a sub-4-bit writing model that beats Q4_K_M on the meaning-preservation benchmark? This is a training project and needs its own approved proposal.

A new quantization format or training method is a training/checkpoint and runtime-kernel change under the project's approval rules. It needs a concrete proposal and explicit approval before work begins; this page does not make that proposal.
