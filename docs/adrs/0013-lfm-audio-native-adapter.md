# ADR-0013: Recognize English with resident LFM2.5 Audio weights

Status: Accepted

Date: 2026-09-15

Approval: [issue #40](https://github.com/rahulsharmavishwakarma/transcribe/issues/40#issuecomment-5685646257). The user approved the proposed adapter after linking the [LFM2.5-Audio-1.5B documentation](https://docs.liquid.ai/lfm/models/lfm25-audio-1.5b). This implementation covers that English checkpoint. Japanese and additional writing checkpoints remain separate qualification work in the issue.

## Decision

Register `lfm2_audio` as a compiled speech architecture using the existing pinned `llama-cpp-2 = 0.1.156` optional `mtmd` feature. The inference worker owns one selected backbone and audio projector. LFM and Qwen share backend initialization, while their weights, contexts, and caches remain independent. Drop the projector before the backbone because native mtmd retains model vocabulary references. Each audio segment creates fresh convolution/attention decode state; no transcript cache or Qwen prefix snapshot is reused. Installation path, artifact size/modification time, and requested backend changes invalidate LFM residency; every load verifies artifact hashes.

The initial supported path is **explicit English, CPU, completed recordings**. Model selection, installation, language controls, diagnostics, and evaluation use the existing application interfaces. `Auto` also selects CPU for this speech adapter. Zero LM GPU layers, an empty device list, disabled context offloading, and `use_gpu=false` for the projector enforce placement. Other accelerators require their own qualification. Cohere → Qwen remains the default.

The existing balanced low-energy segment planner is shared with the other completed-audio adapters. It delivers all samples once, without overlap, deduplication, or quiet-audio removal. The [local comparison](../../benchmarks/asr/2026-09-15-lfm.md) retained ten seconds after 20/30-second trials exposed repeated-speech and cleanup problems. Ten seconds bounds each segment; it does not limit a recording and is not an official LFM duration limit. Exact digital silence produces empty output. This is not incremental live ASR. The projector's full Conformer attention can allocate quadratic intermediates, so a 32K text context is not an audio-duration memory guarantee.

Use the official ASR task framing with one explicit start token and the native media marker. Continuous input audio receives no speech-output modality markers. Count actual multimodal positions before evaluation; cap context at 2048 and reserve up to 512 output tokens per segment. Use greedy sampling and require the end-of-turn token. Reject context exhaustion, output exhaustion, nonfinite logits, unexpected control/audio tokens, and failed segments instead of returning a successful prefix. Qwen cleanup retains the existing raw-transcript preservation and fidelity checks.

Cancellation is checked around audio preprocessing/evaluation and between generated text tokens. The safe mtmd API has no immediate abort callback inside audio encoding. Report observed cancellation latency. Metrics distinguish frontend, context creation, combined audio encoding/LM prefill, generation, and total transcription; the helper does not expose independent encoder/prefill timing without changing its native execution path.

## Evidence and reproducibility

| Evidence class | Basis and boundary |
| --- | --- |
| Primary paper | The [LFM2 report](https://arxiv.org/html/2511.23404v1) describes gated short convolutions, attention, and the continuous audio frontend. This motivates architecture-specific processing/state; its results do not predict this newer Q4 CPU application path. |
| Official model/runtime | The linked model specifications list English despite broader multilingual marketing wording. The [pinned mtmd API](https://docs.rs/llama-cpp-2/0.1.156/llama_cpp_2/mtmd/index.html) and bundled llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621` contain LFM2A audio input. Official [processor](https://github.com/Liquid4All/liquid-audio/blob/19e65845923a7f136442c95137884ec61eb386aa/src/liquid_audio/processor.py) and [generator](https://github.com/Liquid4All/liquid-audio/blob/19e65845923a7f136442c95137884ec61eb386aa/src/liquid_audio/model/lfm2_audio.py) define framing, greedy defaults, and termination. |
| Local measurement | Native installed-model tests cover recognition, output-limit failure, A→B→A, cancellation/reset, silence, sample rate, unload/reload, rejected streaming, and LFM→Qwen cleanup. Quality and performance results belong to the recorded benchmark envelope in issue #40. Passing source/API checks alone does not qualify a checkpoint. |
| Hypothesis | Lower footprint and useful CPU latency on other edge devices remain unverified. Published detokenizer speedups concern speech output and are not ASR speedups. |

English checkpoint: `LiquidAI/LFM2.5-Audio-1.5B@c362a0625dfe45aa588dce5f0ada28a7e5707628`. GGUF export: `LiquidAI/LFM2.5-Audio-1.5B-GGUF@7d525f883a077e20afb782f2ff618edcae0e39e4`. [Artifact manifest](../../models/artifacts.json) pins filenames, sizes, download URLs, and SHA-256 for both Q4 files. Their combined size is 915,262,016 bytes; file size is not RAM. Preserve the [LFM Open License 1.0](https://huggingface.co/LiquidAI/LFM2.5-Audio-1.5B/blob/c362a0625dfe45aa588dce5f0ada28a7e5707628/LICENSE) and checkpoint third-party notices in distribution/provenance.

This is category 1 frozen-weight application work using public APIs. Category 2 work is limited to compiling already bundled mtmd support; no native kernel, decoder, quantization format, or runtime revision is changed. Category 3 training/checkpoint adaptation, speech synthesis, and model promotion are outside this implementation. The [canonical research map](../research-streaming-inference-papers.md#lfm-audio-and-writing-models-up-to-2b) and issue retain the broader candidate inventory.
