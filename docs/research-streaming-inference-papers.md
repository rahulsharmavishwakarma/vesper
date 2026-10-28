# Research basis for streaming inference

Research updated **26 September 2026**; earlier dated reviews retain their own evidence scope. This document maps primary papers to engineering constraints for Vesper. It complements the model-specific [NVIDIA, historical Kyutai, and Moshi review](research-streaming-speech.md), the system-level [device performance plan](inference-performance.md), and the accepted resident-streaming decision in [ADR-0006](adrs/0006-resident-nemotron-streaming-worker.md). Kyutai research is retained as evidence, but [GitHub issue #48](https://github.com/rahulsharmavishwakarma/vesper/issues/48) records its removal from current application support.

## Executive conclusion

The closest paper to the selectable Nemotron model is **Stateful Conformer with Cache-based Inference for Streaming ASR**. Its main inference lesson is to preserve native encoder, convolution, and transducer state while sending non-overlapping audio chunks. For this application, that points to one resident recognizer with a fresh native stream per recording, not repeated whole-file inference or overlapping rolling-window redecoding. NeMo-Speech.cpp already owns these model-specific caches; application code should use its supported stream lifecycle instead of recreating cache logic.

That conclusion now grounds the approved Nemotron implementation: a bounded Rust worker owns one resident recognizer, microphone capture sends non-overlapping chunks to a fresh native stream, and the Qwen writing model retains weights while creating fresh request contexts. It does not validate a latency target, alternate right-context setting, automatic endpointing, lower-bit artifact, or other model adapter.

The papers also establish boundaries:

- Chunk transport, model lookahead, forward time, token emission, endpointing, final flush, cleanup, IPC, and UI rendering are different latency components.
- A smaller PCM packet does not automatically reduce the model's right context or make text stable sooner.
- FastEmit, dual-mode distillation, joint endpointing, TDT, multi-blank transducers, and most quantization-aware methods require a compatible trained checkpoint or runtime. They are not application settings for the current GGUF.
- Paper results are evidence for a technique, not performance promises for Nemotron 3.5 Q8, NeMo-Speech.cpp, Hindi/Hinglish, Apple Metal, or a CPU edge device.

## Evidence rules

The [25 September diarization release review](research-audio-quality.md#nemotron-3-diarization-general-release--25-september-2026)
updates the earlier preview exclusion for the general-release Nemotron 3 model.
It separates the Streaming Sortformer paper, pinned GGUF/native capabilities,
the application's initial missing wiring and unmeasured quality hypotheses. Its
[local CPU/Metal sample follow-up](validation/nemotron-diarization-2026-09-25.md)
now records completed-file timing, speaker diagnostics and a reproducible
10 ms end-boundary failure; it does not measure ASR or live app latency. Native
word tagging and NeMo's per-speaker ASR conditioning are different pipelines;
neither input-buffer latency nor published GPU DER qualifies our Mac path.
The subsequently approved [ADR-0022](adrs/0022-local-speaker-timelines.md) adds
local post-ASR speaker timelines through the standalone C ABI, with bounded tail
validation. The [application validation](validation/nemotron-diarization-2026-09-25.md#application-integration-validation)
separates native/desktop checks from the earlier standalone smoke measurements.

The 16 September personal dictation review (removed in #353; Git history keeps it) extended this map with contextual and phonetic retrieval, TurboBias 2.0, speech-repair understanding, speaker adaptation, active review and continual-learning research. It separates the current public-API personalization from native decoder work and trained checkpoints, and records the exact runtime/model boundaries without claiming new local quality results.

The approved adapter and frozen-weight personalization implementation is described in [ADR-0012](adrs/0012-adapter-registry-and-reviewed-personalization.md). Its [CPU cleanup screening](../benchmarks/cleanup/README.md) separates Prompt Cache/LaMP/contextual-biasing research, pinned runtime capabilities, local measurements and remaining hypotheses. That result measures two-pass cleanup, not streaming algorithmic or user-visible latency.

Every performance claim should be labelled as one of:

| Evidence                      | What it establishes                                                                     | What it does not establish                                            |
| ----------------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Primary paper                 | A technique and results under the paper's models, data, hardware, and metric definition | Support or equivalent results in this repository                      |
| Official model/runtime source | A checkpoint or public API capability at a named revision                               | Useful speed, accuracy, packaging, or correct application integration |
| Local measurement             | Observed behavior for the recorded build, artifact, device, and input                   | A universal default or result on another device/language              |
| Hypothesis                    | A reason to run an experiment                                                           | A feature or performance claim                                        |

Prefer papers and official implementations over summaries. Pin exact revisions when an experiment begins. Preserve negative and failed runs; do not time only successful requests.

## Native live-input feasibility by adapter

Source review **26 September 2026**, against baseline application `6f90343`.
The following tables preserve that source review, not the later implementation
status. The approved common live interface, resident CPU owners, bounded preview
scheduler and Qwen cancellation/backlog patch are now described in
[live adapters](live-transcription.md). Current app support and the historical
two-clip Nemotron replay are in
[model presets](model-presets.md#live-transcription-support). Separate adapters
provide the integration boundary, but do not establish incremental audio support.
A runtime's live-input API can legitimately revise text or recompute a pending
window; it need not have Nemotron's cache semantics. Token output from a completed
audio argument alone does not provide live microphone transcription.

| Selected path                        | Versioned source finding                                                                                                                                                                                                                                                                                 | Integration boundary                                                                                                                                                               |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Nemotron / NeMo-Speech.cpp           | Pinned runtime `4f9676226f667d14608487df744f375db87127f8` has a recognizer/stream lifecycle and cached RNN-T inference.                                                                                                                                                                                  | Already wired; remaining microphone/display and lifecycle qualification is tracked in #3.                                                                                          |
| Qwen3-ASR / MLX                      | Runtime `d302a5c6080d2bb97bae38c7418f82abb76013b6` provides a public session accepting the loaded Qwen model and consecutive audio. See the detailed boundary below.                                                                                                                                     | Candidate extension of the existing Swift/Rust bridge; no replacement checkpoint or dependency required. English/Metal first.                                                      |
| Qwen3-ASR / Sherpa                   | Current `1.13.8` offline adapter processes complete encoder features and creates request-local decoder state.                                                                                                                                                                                            | The MLX session does not enable this CPU/ONNX path.                                                                                                                                |
| Parakeet v3 / Sherpa or MLX          | Selected paths are offline. The pinned [NeMo native runtime](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/4f9676226f667d14608487df744f375db87127f8/docs/asr/models.md) explicitly rejects v3 streaming.                                                                                                | Moving the same weights to that runtime or flipping a registry flag is insufficient.                                                                                               |
| Parakeet Unified / Sherpa            | [Official model card](https://huggingface.co/nvidia/parakeet-unified-en-0.6b) describes joint offline/streaming training, but public streaming recomputes left context. The selected export contains its offline mode.                                                                                   | A different export and qualified runtime path are needed; cache passing remains future work in [paper section 4.3](https://arxiv.org/html/2604.19079v1).                           |
| Parakeet v2 / European FastConformer | Current offline transducer exports have no verified cache-aware input path here. The [European checkpoint](https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/stt_multilingual_fastconformer_hybrid_large_pc_blend_eu) is distinct from NVIDIA's English streaming checkpoint.                 | Keep these profiles' capabilities separate; architecture family names do not qualify them.                                                                                         |
| Cohere / Sherpa                      | [Pinned encoder API](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-cohere-transcribe-model.h) takes full features, without prior encoder state. MLX also offers a runtime-owned live-input session with repeated pending-window encoding. | No live path in the current adapter. The MLX candidate needs compatible weights and renewed quality qualification; its earlier completed-audio comparison added clean-word errors. |
| LFM2.5 Audio / llama.cpp mtmd        | [Released configuration](https://huggingface.co/LiquidAI/LFM2.5-Audio-1.5B/blob/main/config.json) and [pinned native encoder](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/tools/mtmd/models/conformer.cpp) use full-context audio encoding.                      | Streaming generated text/audio does not establish incremental ASR input for the selected Q4 pair.                                                                                  |
| Whisper Small / Turbo                | [Official encoder](https://github.com/openai/whisper/blob/v20250625/whisper/model.py) uses full audio-window attention; the pinned MLX token-stream API takes a completed audio array.                                                                                                                   | Live use needs an explicit buffered decoding design or a different model path; existing output callbacks are insufficient.                                                         |

Selected export revisions, distinct from upstream model-card revisions:
Qwen ONNX `68818b2313fe77bd06f6a7c5068ff3ef59d02b8a`, Qwen MLX
`89e96d92ba34aca20b3e29fb10cc284097d1219f`, Parakeet v3 ONNX
`2bda32ec70b097a55adaa07d9a7173915b43cc78`, v3 MLX
`ed2b7e8c15f9aaa0b5772e2efb986255eaef7e15`, v2
`1ab9323565ddb038682214b292f588070a538ce2`, Unified
`8c3a10fb13408c7a7054f6898958bf1c64a8d6c7`, European
`a68ecf678bbc9369516ca257a8418c8f085771eb`, Cohere
`156a470cf08eefe706a0004f3c52d9ee567ca7a0`, LFM GGUF
`7d525f883a077e20afb782f2ff618edcae0e39e4`, Whisper Small
`8f3c18b358db4d1f2fc1eae49d75cd20989e4309`, Turbo
`2ca6ff69fc878651b770880507669577ac41c2ff`. Artifact hashes remain canonical in
[the manifest](../models/artifacts.json) and [presets](../models/presets.json).
Sherpa source revision is `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`.

Local graph inspection with ONNX Runtime 1.30.0 found only audio/length inputs
and encoded-output/length outputs, without encoder cache tensors, for installed
v3 and Unified encoders. Their SHA-256 values were respectively
`acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247` and
`6716910b7a0833997fec7a410494c995d70124001a0e9b66d6370d6aced577e0`.
This is interface inspection, not ASR or latency measurement; v2 and European
graphs were not inspected locally. A cache-capable re-export or different
checkpoint remains an experiment, not an impossible architecture or an approved
replacement. For example, [Parakeet Realtime EOU 120M](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1)
is a separate English-only streaming candidate without punctuation/capitalization.

The Cohere MLX candidate uses the same pinned session package as Qwen, but
re-encodes growing audio for partials and finalizes approximately 8-second windows.
The [earlier FP16 artifact and comparison](inference-performance.md)
(`beshkenadze/cohere-transcribe-03-2026-mlx-fp16`, revision
`06bbe0d853a93cbbf7c1f40cffccd4c5481459ef`) document six additional clean-word
errors in that completed-audio comparison. Production MLX selection therefore
excludes Cohere. Its live API makes it a research candidate, not a qualified
replacement for the current default's ONNX files or evidence that training is
required. Final termination reporting, window boundaries and lifecycle also
need qualification for this candidate.

### Qwen MLX session: capability and qualification boundary

The pinned [StreamingInferenceSession](https://github.com/Blaizzy/mlx-audio-swift/blob/d302a5c6080d2bb97bae38c7418f82abb76013b6/Sources/MLXAudioSTT/Streaming/StreamingInferenceSession.swift)
is part of the already linked MLXAudioSTT package. It accepts the same loaded
Qwen model through `feedAudio`, emits text snapshots, and exposes asynchronous
Stop plus cancellation. This is the MLX runtime's API; the Qwen team's
[reference implementation](https://github.com/QwenLM/Qwen3-ASR/blob/7c6daf77a2421100f5fb066495372c00129d39ff/qwen_asr/inference/qwen3_asr.py)
has different buffering semantics. The [Qwen report](https://arxiv.org/html/2601.21337v1)
supports chunked recognition and revisable text, not equivalent performance
between these runtimes.

MLX incrementally computes mel features, caches completed acoustic windows,
re-encodes the pending window and starts a fresh decoder cache for each pass.
Defaults use approximately 8-second windows with 1-second overlap and a 1-second
decode cadence. Those configuration values are not first-word latency results.
The baseline completed-audio bridge did not call this session. The approved
live bridge now does, but uses the strict completed-audio pass for final text.

Source inspection identifies production gates: cancellation does not await
inference quiescence; an event stream and completed-window backlog lack explicit
bounds; final text has no decoder termination report; boundary finalization can
reuse partial text; Stop can revise previously confirmed current-window words.
Automatic-language output parsing also differs from completed-audio generation.
These are reasons for targeted tests and narrow runtime corrections, not measured
failures of a Vesper streaming implementation. No Qwen live model run was made
in this review.

**Frozen-weight application scope:** reuse the existing resident model/worker;
add an opaque session to the Swift C ABI and safe Rust wrapper; deliver ordered,
non-overlapping PCM; expose bounded snapshot events; drain Stop before final
delivery and retain ownership through cancellation. Each recording gets fresh
state. Keep the current English/Metal qualification scope, and preserve the
selected model's completed-audio fallback without silently dropping input.

**Runtime-internal scope:** narrowly address cancellation completion, backlog
bounds and final termination reporting where public APIs cannot satisfy the
existing app contract. This requires code and qualification beyond setting a
capability flag. **Training/checkpoint scope:** none for this Qwen candidate;
new Parakeet checkpoints or exports are separate proposals.

**Unverified hypothesis:** this session can provide useful live Qwen output on
the current Mac within acceptable memory, latency and quality limits. Qualify
with paced public fixtures against the same model's completed-audio output:
first-partial and word-emission timing, Stop-to-final, WER/CER and meaning,
partial revisions, silence/quiet speech, repeated phrases, short tails and speech
across 7/8/14/15-second boundaries. Exercise cancellation/restart, overflow and
sustained memory; then measure native microphone-to-visible behavior. Record
the full environment and failures under the metrics below. Existing Nemotron
results do not qualify Qwen. The concrete proposal and approval status belong
in [issue #3](https://github.com/rahulsharmavishwakarma/vesper/issues/3).

## Paper-to-engineering map

### Shared live interface for completed-audio adapters

The 26 September source review extends the integration target to **every current
ASR adapter**, including CPU paths and existing transducer profiles. One session
contract can expose input, provisional snapshots, finalization and cancellation
while keeping model-specific behavior behind the adapter. Native input streaming
and buffered previews need distinct capability metadata; backend/language support
continues to depend on the exact model, artifact and runtime.

The reviewed baseline internal API returned `vesper_nemo::Stream`, and both the
manager and desktop recording entry point gated on `native_streaming`. The
implementation now generalizes those boundaries and the evaluator alongside
adapter methods. An asynchronous session must distinguish **pending output** from
**finished**; an empty poll is not proof of finalization. Existing operation IDs,
monotonic event sequences and replacement-snapshot UI semantics can be reused.

For completed-audio runtimes, bounded windows can supply provisional previews
using the existing weights and decoder. [Whisper-Streaming](https://arxiv.org/html/2307.14743v2)
establishes the feasibility of repeated decoding and agreement-based output; its
safe buffer trimming additionally uses word timing that Vesper's text-only
adapter results do not currently expose. It does not establish local quality,
latency or equivalence to NVIDIA's native buffered RNN-T implementation.

The conservative finalization boundary is therefore an explicit request for the
existing completed-recording pass after Stop. Preview text remains revisable;
only the successful final pass feeds cleanup, persistence, learning and insertion.
This adds preview compute and retains offline Stop-to-final cost. Current segment
boundaries depend on total recording length, so concatenated preview strings must
not be treated as equivalent to the existing final result. Repeated words and
window boundaries require explicit tests rather than blind text deduplication.

Slow synchronous decode can fill the existing 32-chunk channel and interrupt live
delivery. Scheduling needs one running preview and one newest pending request,
with obsolete preview requests coalesced independently of retained recording
audio. Preserve every captured sample for final processing, including when live
updates cannot keep up. CPU Qwen-ASR and Parakeet reloaded per job in that baseline; useful
previews require retaining their recognizer at least across one recording's
preview passes, with fresh decoder state and artifact validation. This is a model
lifetime change as well as an adapter extension. Existing resident Cohere,
Whisper, LFM and MLX owners can be reused.

The full proposal, including lifecycle and scheduling changes, is recorded in
[issue #3](https://github.com/rahulsharmavishwakarma/vesper/issues/3).
The [device resource boundary](inference-performance.md#resources-for-live-adapters)
documents the implemented resource controls and remaining hardware qualification.
All-model pre-Stop updates, final-result parity, cancellation, bounded queues,
language constraints and sustained memory require per-backend qualification;
this source analysis is not an implemented or measured all-model live path.

### Directly relevant to the default cache-aware RNNT

| Primary source                                                                                                   | Technique and evidence                                                                                                                                                                                                                                              | Applicability here                                                                                                                                                                                                                                                          |
| ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Graves, Sequence Transduction with Recurrent Neural Networks (2012)](https://arxiv.org/abs/1211.3711)           | RNN-T marginalizes monotonic alignments and combines acoustic state with output-history state, enabling incremental decoding without frame-level labels.                                                                                                            | Foundational only. Preserve encoder and predictor/decoder state inside an utterance; reset it between recordings. The original TIMIT experiment is not a modern latency benchmark, and RNN-T loss alone does not force early emission.                                      |
| [Zhang et al., Transformer Transducer (2020)](https://arxiv.org/abs/2002.02562)                                  | Limited left attention plus a small amount of future context demonstrates a real quality/latency tradeoff.                                                                                                                                                          | Expose only right-context values supported by the exact Nemotron artifact/runtime. Left-context cache controls compute and memory; right context adds algorithmic delay. This is not a switch for unrelated checkpoints.                                                    |
| [Shi et al., Emformer (2021)](https://arxiv.org/abs/2010.10759)                                                  | Block processing, a memory bank, and cached attention K/V avoid recomputing left context. The paper reports an 18% relative decoding-RTF reduction over its AM-TRF baseline and separate 80 ms/960 ms algorithmic-latency operating points.                         | Supports resident state and non-overlapping block inference as principles. Emformer is a different encoder; its cache format and results must not be transplanted into FastConformer or quoted as application latency.                                                      |
| [Noroozi et al., Stateful Conformer with Cache-based Inference (2024)](https://arxiv.org/abs/2312.17279)         | Bounded past/lookahead and cached attention, depthwise-convolution, and RNNT state eliminate overlapping buffered recomputation. Multi-lookahead training supports multiple latency points.                                                                         | Closest basis for Nemotron. Feed non-overlapping chunks, keep stream state through `finish`, and let the native runtime own caches. The paper's latency excludes neural forward time, used smaller paper models, and does not predict desktop, multilingual, or UI latency. |
| [Rekesh et al., Fast Conformer (2023)](https://arxiv.org/abs/2305.05084)                                         | More aggressive subsampling and redesigned attention reduce encoder compute; the paper reports a 2.8x speedup over its original-Conformer comparison.                                                                                                               | Preserve the checkpoint's exact frontend, subsampling, and runtime semantics. Capture packet duration is not necessarily the encoder frame or emitted-token cadence.                                                                                                        |
| [Banfic et al., Pushing the Limits of On-Device Streaming ASR (2026 preprint)](https://arxiv.org/abs/2604.14493) | Benchmarks more than 50 ASR configurations and reports an ONNX Nemotron English implementation combining graph fusion with post-training quantization; the recommended int4 variant is reported at 0.67 GB and 8.20% average streaming WER across eight benchmarks. | Justifies a future Q4/int4 experiment after a resident Q8 baseline. It does not validate multilingual Nemotron 3.5, the NeMo-Speech.cpp GGUF path, Hindi/Hinglish, Metal, or this app's end-to-end latency.                                                                 |

The official [Nemotron 3.5 model card](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b) identifies a prompt-conditioned, cache-aware FastConformer-RNNT and documents `[56,R]` settings where `R = 0,1,3,6,13` corresponds to 80, 160, 320, 560, and 1120 ms chunks. The 56 cached past frames are context, not an additional 4.48-second wait. Confirm the values supported by the pinned native runtime instead of assuming every interface exposes every model-card setting.

The pinned [NeMo-Speech.cpp 0.1.0 SDK](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.1.0/docs/sdk.md) defines the lifecycle: create a recognizer, create a stream, alternate audio pushes and result polling, finish, drain, and close. A recognizer may serve independent calls and must outlive its streams; an individual stream is single-threaded. Its [ASR configuration](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.1.0/docs/asr/configuration.md) distinguishes RNNT right context from CTC buffered-window settings and notes batching controls that can add queue delay. These are the authoritative integration contracts.

### Partial hypotheses and endpointing

| Primary source                                                                                                                                                                                             | Technique and evidence                                                                                                                                                                               | Applicability here                                                                                                                                                                                                                                |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Shannon et al., Improved End-of-Query Detection (2017)](https://www.isca-archive.org/interspeech_2017/shannon17_interspeech.pdf)                                                                          | Shows that silence classification is not the same as deciding the speaker has finished; fillers, prior rate, and rhythm can indicate continuation. It evaluates latency together with cutoff errors. | Keep manual Stop authoritative initially. Evaluate cutoff rate plus endpoint p50/p90 on pauses, fillers, slow speech, noise, Hindi, and code-switching before automatic closure. Voice-search results do not transfer directly to long dictation. |
| [Chang et al., Temporal Modeling Using Dilated Convolution and Gating for VAD (2018)](https://research.google/pubs/temporal-modeling-using-dilated-convolution-and-gating-for-voice-activity-detection/)   | Reports recurrent VAD-state saturation on long dictation and uses a causal dilated gated CNN to avoid periodic state resets.                                                                         | Treat long-running VAD state as a bounded lifecycle requiring explicit reset tests. A long user session and one ASR/VAD segment need not be the same object.                                                                                      |
| [Bijwadia et al., Unified End-to-End Speech Recognition and Endpointing (2022)](https://research.google/pubs/unified-end-to-end-speech-recognition-and-endpointing-for-fast-and-efficient-speech-systems/) | Joint ASR/endpoint training reuses encoder representations; its voice-search evaluation reports 130 ms lower median and 160 ms lower p90 endpoint latency without WER regression.                    | Training/model-output feature, not a bolt-on runtime optimization. Use only when a compatible checkpoint and adapter expose endpoint evidence.                                                                                                    |
| [Shangguan et al., Quality and Stability of a Streaming On-Device Recognizer (2020)](https://www.isca-archive.org/interspeech_2020/shangguan20_interspeech.pdf)                                            | Defines unstable partial-word and segment metrics (UPWR/UPSR) and identifies punctuation, casing, spacing, and numerals as major flicker sources.                                                    | Instrument exact partial revisions before choosing UI policy. Persist the exact final raw ASR output; do not treat cosmetic partial changes as committed.                                                                                         |
| [Liu et al., Partial Hypothesis Selection (2020)](https://www.isca-archive.org/interspeech_2020/liu20s_interspeech.pdf)                                                                                    | Local Agreement commits the longest common prefix across consecutive chunk hypotheses; its seq2seq experiment found a useful latency/quality tradeoff.                                               | Use an append-only committed prefix plus replaceable provisional suffix for revisionable adapters. Do not add rolling-window redecoding to a monotonic native transducer that already exposes finalized deltas.                                   |
| [Macháček et al., Turning Whisper into a Real-Time Transcription System (2023)](https://aclanthology.org/2023.ijcnlp-demo.3/)                                                                              | Whisper-Streaming reprocesses a rolling buffer, applies LocalAgreement-2, trims at stable boundaries, and retains prompt context.                                                                    | Useful fallback pattern for offline seq2seq models such as Whisper, not the native Nemotron design. Its reported GPU latency and repeated decoding do not establish CPU/edge performance.                                                         |

The [Whisper qualification review](research-whisper.md) applies these distinctions to the pinned Sherpa decoder, its native limits, and the separate whisper.cpp candidate. Running the isolated probe does not implement Whisper-Streaming.

An endpoint event is not necessarily “all text arrived.” Correct finalization stops capture, pushes every queued PCM frame, signals end of input, drains delayed tail results, and only then emits a final transcript. Cleanup and training-data persistence should consume final raw text, not mutable partials.

The implemented event contract carries an operation ID, monotonically increasing sequence, committed text, provisional text, consumed audio time, emission time, and final flag. The UI accepts only the active operation and a newer sequence. A segment ID is deferred because automatic multi-segment endpointing is not implemented.

### Kyutai delayed streams and Moshi

| Primary source                                                                                                                     | Technique and evidence                                                                                                                                                       | Applicability here                                                                                                                                                                                                                                             |
| ---------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Zeghidour et al., Streaming Sequence-to-Sequence Learning with Delayed Streams Modeling (2025)](https://arxiv.org/abs/2509.08753) | Aligns audio and text streams on an 80 ms grid and deliberately delays text to trade latency for future acoustic context. A finite context supports arbitrarily long inputs. | This is the relevant basis for Kyutai STT. The 1B public model's English/French scope, fixed delay, and semantic VAD must be benchmarked through its own stateful runtime. H100 throughput and English evaluation do not establish CPU, Hindi, or app latency. |
| [Défossez et al., Moshi (2024)](https://arxiv.org/abs/2410.00037)                                                                  | Models user audio, assistant audio, and text as parallel streams for full-duplex spoken dialogue.                                                                            | Moshi's 160 ms theoretical/about 200 ms practical headline concerns generated conversational audio on an L4, not ASR word emission, endpoint, or dictation finalization. Do not use it as this product's ASR target.                                           |

Kyutai's [official delayed-streams implementation](https://github.com/kyutai-labs/delayed-streams-modeling) advertises about 0.5-second delay and semantic VAD for `stt-1b-en_fr`, while `stt-2.6b-en` uses about 2.5 seconds. Treat semantic VAD quality as an implementation claim until cutoff and endpoint distributions are reproduced for this workload.

### LFM audio and writing models up to 2B

Research checked **15 September 2026**. The concrete integration scope, approval status, artifact hashes, and qualification criteria live in [issue #40](https://github.com/rahulsharmavishwakarma/vesper/issues/40). The approved English CPU adapter is implemented in [ADR-0013](adrs/0013-lfm-audio-native-adapter.md); see [local qualification](../benchmarks/asr/2026-09-15-lfm.md). Other LFM variants remain candidates.

| Evidence class              | Finding and engineering consequence                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Primary paper               | The [LFM2 technical report](https://arxiv.org/html/2511.23404v1) describes gated short convolutions plus grouped-query attention and continuous audio input through a FastConformer encoder. Native architecture-specific state must remain isolated between utterances. Its results do not establish latency for this application's LFM2.5 Q4 CPU path.                                                                                                                                                     |
| Official model capability   | Liquid lists separate [English and Japanese 1.5B audio checkpoints](https://docs.liquid.ai/lfm/models/audio-models); these are the smallest current official LFM speech models found. [LFM2.5-230M](https://huggingface.co/LiquidAI/LFM2.5-230M) is text-only and is a separate writing candidate. Apply the ceiling to total parameters; an 8B model with 1B active parameters does not qualify.                                                                                                            |
| Official runtime capability | The existing `llama-cpp-2 = 0.1.156` exposes optional [mtmd audio APIs](https://docs.rs/llama-cpp-2/0.1.156/llama_cpp_2/mtmd/index.html). Its bundled llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621` contains [LFM2A input handling](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/tools/mtmd/mtmd.cpp). This supports an ASR-only integration experiment using the backbone and matching projector, without assuming the full speech-output runner is required. |
| Local measurement           | [English CPU screening](../benchmarks/asr/2026-09-15-lfm.md) records native correctness, held-out accuracy, repeated requests, boundary tests and a synthetic soak. These measurements do not qualify other languages, checkpoints or devices.                                                                                                                                                                                                                                                               |
| Hypothesis                  | Resident Q4 weights and bounded audio segments may make LFM useful on CPU. Compare the same held-out English recordings against current adapters, and evaluate each writing checkpoint's meaning preservation before promotion.                                                                                                                                                                                                                                                                              |

The approved application work uses frozen weights and public APIs: shared backend initialization, resident backbone/projector on the existing worker, fresh per-segment decode state, exact ASR framing, context/output budgets, and completion checks. The projector must be dropped before its backbone. The official [processor](https://github.com/Liquid4All/liquid-audio/blob/19e65845923a7f136442c95137884ec61eb386aa/src/liquid_audio/processor.py) and [generation implementation](https://github.com/Liquid4All/liquid-audio/blob/19e65845923a7f136442c95137884ec61eb386aa/src/liquid_audio/model/lfm2_audio.py) define the task format and text termination; generating an audio modality token is an ASR failure, not transcript text. The ownership change was explicitly approved in issue #40.

The pinned [audio frontend](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/tools/mtmd/mtmd-audio.cpp) processes complete 16 kHz mono input. Its [Conformer attention](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/tools/mtmd/models/conformer.cpp) can allocate quadratic intermediates. A 32K LM context is not a safe audio-duration guarantee. Reusing the existing completed-audio segment planner with an initial 10-second segment bound is an engineering experiment, not an official LFM limit or live streaming ASR. Measure boundary omissions/duplications, whole-recording coverage, and cancellation latency; the safe wrapper exposes no immediate abort inside audio encoding.

Runtime-internal work is limited in this proposal to enabling the already bundled mtmd build feature. New kernels, cache formats, runtime upgrades, and accelerator claims require separate evidence and scope. Training/checkpoint changes, including quantization-aware distillation, are separate from ordinary Q4 inference; no training is proposed. Liquid's advertised [8× mobile improvement](https://www.liquid.ai/blog/introducing-lfm2-5-the-next-generation-of-on-device-ai) concerns audio detokenization, not ASR or two-stage application latency. Keep English and Japanese qualification separate, preserve LFM Open License provenance, and retain Cohere → Qwen as the selected default.

### Gemma 4 E2B: audio to finished text qualification

| Evidence class            | Finding and boundary                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Primary paper             | The [Gemma 4 report, §§2–2.2](https://arxiv.org/html/2607.02770v1) describes a 305M USM/Conformer audio encoder, continuous audio embeddings, E2B's 4:1 local/global attention and shared KV layers. Its global key=value reuse explicitly **excludes E2B/E4B**. The reported global-cache reduction and mobile memory figures must not be assigned to this E2B GGUF experiment.                                                                                                                                                                       |
| Official capability       | [Google's card](https://ai.google.dev/gemma/docs/core/model_card_4) defines multimodal input and text output, with E2B approximately 2.3B effective / 5.1B total parameters. The [audio guide](https://ai.google.dev/gemma/docs/capabilities/audio) specifies 16 kHz mono float32 in [-1,1], at most 30 seconds. An “any-to-any” task tag does not establish speech output or streaming.                                                                                                                                                               |
| Official runtime/artifact | Google's [QAT Q4_0 GGUF](https://huggingface.co/google/gemma-4-E2B-it-qat-q4_0-gguf/tree/675cff42a74c774d6cb76f76d8eacb49b48c9b93) includes a separate projector. Existing llama-cpp-2 0.1.156 / llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621` implements [Gemma4 audio](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/tools/mtmd/models/gemma4a.cpp). Native per-chunk evaluation supplies the appropriate attention mode; do not infer audio causality from the safe wrapper's chunk-less vision query. |
| Local measurement         | The [development-only comparison](../benchmarks/asr/2026-09-16-gemma4.md) separates ASR, direct minimal polish, paired raw/polished output, and existing two-model execution. It does not register a product adapter or qualify other devices/languages.                                                                                                                                                                                                                                                                                               |
| Hypothesis                | One generation may remove a pipeline invocation, but can increase decoder work, memory or content errors. A raw field generated alongside polished text is not an independent fidelity check. Compare quality and total completion time before integration.                                                                                                                                                                                                                                                                                            |

The frozen-weight experiment uses the pinned [source template](https://huggingface.co/google/gemma-4-E2B-it/blob/3e22461f65e89153144f8adb70e3b8c2cc9845a7/chat_template.jinja), thinking disabled, text before audio, fresh CPU contexts and valid completion checks. It uses existing public native APIs only. New kernels, runtime changes, cache formats, and accelerated execution belong to runtime-internal work; none are introduced here. QAT belongs to training/checkpoint provenance, not a new application setting. No training is performed. The test-first scope and any subsequent integration decision are tracked in issue #41.

The CPU profile identified audio encoding as the dominant cost with Google's BF16 projector. The pinned [CPU GEMM dispatch](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/ggml/src/ggml-cpu/llamafile/sgemm.cpp) supports ARM F16 paths without an equivalent BF16 path. A separately hashed F16 derivative was tested through existing conversion APIs, preserving every original F32 tensor. This is a supported artifact experiment, not a newly implemented kernel. The [conversion implementation](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/src/llama-quant.cpp) also exposed a four-dimensional conversion assertion; the failed attempt and verified public-override workaround are retained in the benchmark record. Lower latency did not establish equivalent output text, lower memory, or a production qualification.

### Methods that are not frozen-weight application switches

| Primary source                                                                                   | Method                                                                                                                                | Required change                                                                  |
| ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| [Yu et al., FastEmit (2021)](https://arxiv.org/abs/2010.11148)                                   | Sequence-level emission regularization favors earlier non-blank transducer emissions; reported reductions are model/dataset-specific. | Training or fine-tuning and a new evaluated checkpoint.                          |
| [Yu et al., Dual-mode ASR (2021)](https://arxiv.org/abs/2010.06030)                              | Joint streaming/full-context training with shared weights and in-place knowledge distillation.                                        | Training recipe and checkpoint support.                                          |
| [Mahadeokar et al., Alignment Restricted RNN-T (2021)](https://arxiv.org/abs/2011.03072)         | Restricts the transducer lattice using external alignments to bound delay.                                                            | Alignments, retraining, and an explicit WER/latency tradeoff.                    |
| [Shinohara and Watanabe, Minimum Latency Training (2022)](https://arxiv.org/abs/2211.02333)      | Adds expected emission latency to the transducer objective.                                                                           | Retraining; the WSJ result is not evidence for multilingual dictation.           |
| [Xu et al., Multi-blank Transducers (2022)](https://arxiv.org/abs/2211.03541)                    | Learned multi-frame blank symbols skip encoder frames.                                                                                | Compatible output heads, training, checkpoint, and decoder.                      |
| [Xu et al., Token-and-Duration Transducer (2023)](https://proceedings.mlr.press/v202/xu23g.html) | Predicts token and duration so decoding can skip frames.                                                                              | A TDT-trained model and TDT runtime; it cannot be enabled on an RNNT GGUF.       |
| [Yao et al., Zipformer (2023)](https://arxiv.org/abs/2310.11230)                                 | Lower frame rates and reused attention weights make a different encoder family more efficient.                                        | A different model/checkpoint and adapter, not an optimization flag for Nemotron. |

Decoder research such as [Label-Looping](https://arxiv.org/abs/2406.06220), [CUDA conditional-graph RNN-T decoding](https://arxiv.org/abs/2406.03791), and [WIND](https://arxiv.org/abs/2505.13765) may reduce transducer decoder overhead. These techniques require internal tensor/decoder access or upstream runtime support. Their batched/CUDA paper results do not justify forking an opaque native runtime for a single CPU or Metal stream before profiling identifies the decoder as the bottleneck.

Generic techniques also require runtime-specific proof. [Integer-only quantization](https://arxiv.org/abs/1712.05877) establishes that co-designed quantization and kernels can improve mobile latency, but its MobileNet results do not prove that any Q4/Q8 speech GGUF will be faster or accurate. [FlashAttention](https://arxiv.org/abs/2205.14135) is an IO-aware exact-attention algorithm, but it helps only when the selected model/backend implements a compatible kernel and attention is a measured bottleneck. Neither can be added at the Tauri orchestration layer.

### Edge inference and cleanup-model research

The first cleanup optimization is model lifetime, not a new compression format: keep Qwen's immutable `LlamaModel` resident and give each transcript isolated context/state. A reusable prompt prefix is a later experiment; it must contain only the fixed system/template prefix, be keyed by the exact model and chat template, and never restore another user's transcript state.

| Primary source                                                                                                                             | Evidence                                                                                                 | Applicability here                                                                                                                                                                                  |
| ------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [GPTQ (2023)](https://arxiv.org/abs/2210.17323)                                                                                            | Accurate 3–4-bit weight-only LLM quantization with GPU speedups in the paper's runtime.                  | The current Qwen Q4_0 GGUF is not GPTQ. Adopting GPTQ requires a compatible conversion, runtime, and kernels.                                                                                       |
| [AWQ (2024)](https://arxiv.org/abs/2306.00978)                                                                                             | Activation-aware 4-bit weight-only quantization and TinyChat kernels reported desktop/mobile GPU gains.  | AWQ calibration and kernels are not interchangeable with llama.cpp Q4_0. Treat it as a separate model/runtime candidate.                                                                            |
| [SmoothQuant (2023)](https://arxiv.org/abs/2211.10438)                                                                                     | Moves activation difficulty into weights to enable W8A8 post-training quantization.                      | Server-model evidence; the current llama.cpp/GGUF adapter does not expose SmoothQuant directly.                                                                                                     |
| [Prompt Cache (2024)](https://arxiv.org/abs/2311.04934)                                                                                    | Reuses attention state for shared prompt modules to reduce time to first token in long-prompt workloads. | Potentially applicable to the invariant cleanup instruction after resident-model baseline. The absolute gain may be small for a 0.8B model and short prompt. Test isolation and invalidation first. |
| [KIVI (2024)](https://arxiv.org/abs/2402.02750)                                                                                            | Two-bit asymmetric KV-cache quantization reduces memory and improves high-batch/long-context throughput. | A memory-pressure experiment, not an initial latency optimization for one cleanup request capped at 4096 tokens.                                                                                    |
| [PagedAttention/vLLM (2023)](https://arxiv.org/abs/2309.06180) and [Orca (2022)](https://www.usenix.org/conference/osdi22/presentation/yu) | Improve multi-request serving throughput through KV memory management and iteration-level batching.      | Deliberate queue/batch waits can worsen a single user's latency. YAGNI until concurrent cleanup requests are an observed requirement.                                                               |
| [Distil-Whisper (2023)](https://arxiv.org/abs/2311.00430)                                                                                  | Distills Whisper into a smaller English ASR model with paper-specific speed/quality results.             | A separate trained model family, not compression that can be applied to Nemotron, Kyutai, or Qwen weights.                                                                                          |

Runtime documentation determines whether a paper technique is reachable. NeMo-Speech.cpp's pinned [model/quantization guide](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.1.0/docs/asr/models.md) describes Q4_K/Q5_K/Q6_K tradeoffs and partial F16 fallback; its [batching guide](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.1.0/docs/development/asr-batching.md) keeps batching opt-in because queue formation adds latency. For one desktop stream, benchmark Q8 without batching first. The lower-bit profile must pass English, Hindi, Hinglish, entities, numbers, and noise evaluation before promotion.

For the offline Parakeet fallback, a resident recognizer can amortize ONNX session creation, but it does not turn an offline checkpoint into native streaming. ONNX Runtime's official [threading](https://onnxruntime.ai/docs/performance/tune-performance/threading.html), [graph optimization](https://onnxruntime.ai/docs/performance/model-optimizations/graph-optimizations.html), [I/O binding](https://onnxruntime.ai/docs/performance/tune-performance/iobinding.html), and [quantization](https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html) guidance support experiments only after the selected execution provider and exact graph are verified. Sweep threads while capture is active; requested GPU/provider names are not evidence that operators executed there.

For Kyutai, DSM already requires persistent text-model, Mimi, and streaming state. Replacing the complete-file CLI with its stateful library/server path could remove reload and expose words/VAD, but this is a separate approved integration. Its official public artifacts and CPU/Metal/CUDA paths must be measured as published; the existence of a loader in source is not a reproducible, quality-qualified quantized checkpoint.

For Qwen, benchmark resident fresh-context inference before prefix caching, KV-cache quantization, or explicit attention changes. Record whether the pinned llama.cpp runtime already activates FlashAttention automatically. Sweep prompt-processing and generation thread counts separately, and compare current logical/physical batch sizes using real cleanup prompts. Do not batch cleanup across partial captions; run it on final raw text.

### Recent small VAD, noise suppression and audio classification

The [audio-quality investigation](research-audio-quality.md), [initial measurements](validation/audio-frontends-2026-09-16.md) and [hard regressions](validation/audio-regression-2026-09-16.md) connect the [DPDFNet paper](https://arxiv.org/abs/2512.16420v1), [CED paper](https://arxiv.org/abs/2308.11957v2), and [enhancement-artifact/ASR study](https://arxiv.org/html/2404.14860v1) to pinned `sherpa-onnx = 1.13.8` frontend APIs. The user's current shortlist requires model releases from 2025-09-16 onward and allows up to 200 MB per model, with smaller models welcome. Old Silero v4/GTCRN/CED tests are baselines, not eligible recent-checkpoint recommendations. Silero v6.2's November 2025 weight update is distinct from its 2026 packaging release. File size is not memory or latency evidence.

Frozen-weight experiments compare VAD decisions, enhancement, reset, context/tail handling and waveform alignment. Noise-only metrics now avoid conventional empty-reference WER. Efficient Fourier transforms and avoiding growing VAD buffer copies are native-runtime work; artifact-aware training, distillation and personal speaker extraction require checkpoint changes. The approved [shared frontend integration](adrs/0019-shared-audio-frontend.md) now applies advisory Silero v6.2 and optional DPDFNet4/8 through one resident worker stage, with fresh recording state, deterministic framing, aligned tails and original-audio fallback. Both settings remain off by default. Its narrow native endpoint-range guard fixes invalid VAD extraction after bookkeeping flushes; it does not introduce a decoder or FFT kernel. Earlier experiment results do not qualify this application path. Ear identifies language and Uhm identifies candidate filler spans; neither supplies noise removal or persistent user intent. [Issue #42](https://github.com/rahulsharmavishwakarma/vesper/issues/42) contains the proposal and approval status.

## Research-based experiment plan

### Apple Metal, MLX and CUDA qualification

The [22 September accelerator audit](inference-performance.md#historical-mac-gpu-audit--22-september-2026)
maps these research principles to exact current runtime pins, local device
observation, existing writer measurements and proposed integration boundaries.
MLX is a framework with Metal and Linux CUDA paths; it is not an ONNX/GGUF
provider string. Upstream model ports and artifact formats require independent
qualification.

| Evidence class              | Finding and engineering consequence                                                                                                                                                                                                                                                                                                                                                                                                              |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Paper result                | [FlashAttention](https://arxiv.org/abs/2205.14135) reduces intermediate attention memory traffic using tiled exact attention. [FlashAttention-3](https://arxiv.org/abs/2407.08608) exploits Hopper-specific features. Neither paper establishes Apple latency, LFM relative-position fusion, or execution in this repository.                                                                                                                    |
| Official runtime capability | Pinned llama.cpp defaults to Flash Attention AUTO and provides Metal quantized/tensor kernels; pinned NeMo separately gates fused relative attention and convolution on CUDA. Inspect actual dispatch instead of treating “enable attention optimization” as a new feature.                                                                                                                                                                      |
| Local measurement           | The September 22 model-free probe found Apple M5 Pro Metal support. The September 23 native LFM comparison completed 84 cases per backend; resident median ASR was 344.25 ms CPU and 167.58 ms Metal, with 5 differing transcripts. Isolated MLX Whisper/Qwen model runs and failure cases are recorded in the performance guide. Neither these runs nor the earlier writer screen establishes sustained thermals or every operator’s placement. |
| Unverified hypothesis       | Whole-application gains for MLX speech, CoreML graph coverage, CUDA behavior on NVIDIA hardware, and automatic model/backend selection remain unqualified. Candidate ports, framework GPU support and a successful build do not establish those outcomes.                                                                                                                                                                                        |

**Frozen-weight application/runtime work:** measure supported Metal/CUDA paths,
report fallback honestly, use existing offload APIs, and qualify exact MLX
loader/artifact pairs. LFM and the writer now share detected native device selection and complete initialization fallback. The MLX application integration in [ADR-0020](adrs/0020-optional-mlx-speech-runtime.md) retains its English/model gates. [ADR-0021](adrs/0021-automatic-accelerator-artifacts.md) adds approved automatic selection of compatible artifacts and normal Apple Silicon packaging; it does not change the selected default model. Isolated measurements remain separate from application-path validation.
**Runtime-internal work:** the MLX qualification patch exposes actual decoder termination and native Whisper confidence without changing greedy token selection. Its silence policy follows [OpenAI decoding](https://github.com/openai/whisper/blob/v20250625/whisper/decoding.py) and [transcription](https://github.com/openai/whisper/blob/v20250625/whisper/transcribe.py); the probability is measured at the SOT row before filtering, and average generated-token log probability includes EOS. Qwen additionally reuses the existing Whisper frontend and corrects integer audio-token geometry to match the [official pinned processor](https://github.com/QwenLM/Qwen3-ASR/blob/7c6daf77a2421100f5fb066495372c00129d39ff/qwen_asr/core/transformers_backend/processing_qwen3_asr.py). Native geometry tests and the same 84-case rerun passed; the original quality regression and the combined correction's measured improvement are retained in the performance guide. No threshold was tuned on that holdout. This does not imply a new attention implementation. Fused Metal relative-position attention, a GPU-resident
RNNT loop, new quantized packing or unsupported CoreML provider controls require
native changes; a configuration label cannot supply them.
**Training/checkpoint work:** distillation, endpoint training and a different
encoder remain separate model changes. Existing AWQ/KIVI/Prompt Cache evidence
above must not be relabelled as techniques implemented by current Q4_0/F16 state.

### Baselines

Compare the same frozen audio and references across:

1. Legacy complete-file, per-job adapter when reproducing the old baseline.
2. Current resident recognizer with complete-file inference.
3. Current resident recognizer with native streaming at the official runtime default, then separately approved experiments at adjacent supported right-context values.
4. CPU and Apple Metal on the available machine; named CUDA and Vulkan systems only when those devices are actually available.
5. Q8 first, then a separately pinned lower-precision artifact if quality and runtime compatibility justify it.

Replay recorded audio at wall-clock pace for live-stream measurements. Pushing a file as quickly as the machine accepts it measures recorded-stream throughput, not first-partial behavior during capture.

### Required metrics

| Metric                           | Definition                                                                                                                                                           |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Capture-to-first-visible partial | Speech onset and first captured sample should both be recorded as anchors; end at first non-empty UI render.                                                         |
| Word-emission delay              | Time a committed/visible word appears minus its forced-aligned acoustic end; report p50/p90/p99 and alignment method.                                                |
| PR50/PR90                        | Last token emission for a recognition result minus true speech end.                                                                                                  |
| EP50/EP90 and cutoff rate        | Endpoint decision minus true speech end; negative early endpoints are cutoff cases and must not disappear into an average.                                           |
| Stop-to-final                    | User Stop or accepted endpoint to final UI render, decomposed into queue drain, tail flush, postprocessing, IPC, and render.                                         |
| Partial stability                | UPWR/UPSR, optionally partial WER, with normalized and exact-text views distinguished.                                                                               |
| Throughput                       | Define the real-time-factor direction explicitly; also report audio processed per wall-clock second.                                                                 |
| Quality                          | WER with declared normalization, CER for appropriate scripts, punctuation/case separately, and entity/number/negation preservation after cleanup.                    |
| Resources/reliability            | Queue residence and drops, cold and resident load, RSS/VRAM/shared memory, CPU/GPU utilization, power mode, thermal drift, failures, and reset/cancel state leakage. |

Record the application revision, model/runtime revision and hashes, backend, precision, driver, thread settings, chunk/right context, endpoint policy, corpus revision, locale, audio duration, sample count, and failures with every run. English results must not stand in for Hindi or Hinglish. Include pauses, fillers, self-corrections, slow speech, code-switching, names, numbers, noise, silence, and words ending exactly at Stop.

### Lifecycle and correctness gates

- Run at least 100 consecutive create-stream, finish, cancel, and reset cycles on one resident recognizer; prove that text and hidden state do not leak across recordings.
- Bound PCM queues and report dropped or delayed audio. Test capture while CPU/GPU inference is saturated.
- On Stop, flush resampling and every queued frame before signaling end of input; drain all native results before publishing `final`.
- Keep manual Stop authoritative until endpoint cutoff, EP50/EP90, and long-dictation behavior pass the multilingual corpus.
- Keep the cleanup LLM off mutable partials initially. Preserve final raw ASR before cleanup, consistent with [ADR-0004](adrs/0004-persist-raw-asr-before-cleanup.md).
- Reject a faster configuration if it regresses ASR quality, partial stability, cancellation, state isolation, capture reliability, or meaning preservation beyond an approved threshold.

## Implemented baseline boundary

The approved Phase 1 baseline implements these responsibilities:

- Rust owns bounded audio transport, session/segment identity, cancellation, persistence, timings, and event delivery.
- NeMo-Speech.cpp C++ owns model-specific CPU/Metal/CUDA/Vulkan kernels, recognizer weights, encoder caches, and RNNT decoding behind its stable C ABI.
- One bounded worker owns each resident native recognizer. Each recording gets fresh stream state; model switching occurs only at a safe idle boundary.
- Nemotron partial display uses its native result stream and a replaceable provisional field; no generic LocalAgreement layer has been added.
- Qwen cleanup runs on final raw text first. Any future speculative cleanup requires separate evidence and provenance.

The architectural proposal and approval are recorded in GitHub issues #2 and #3 and in ADR-0006. The 100-cycle lifecycle gate, live wall-clock latency distributions, Hindi/Hinglish corpus quality, device memory, and cross-platform accelerator qualification remain open validation—not implied successes of this baseline.

## VibeVoice streaming and heterogeneous CPU quantization

The [VibeVoice family review](research-vibevoice.md) maps the September 2026 streaming report and July 2026 BitNet report to the exact released checkpoints and runtimes. Streaming-1.5B exceeds 2B total parameters, and its algorithmic delay excludes compute. BitNet is a separately trained quantized checkpoint with specialized kernels, not a configuration switch for streaming weights. The review distinguishes public-API experiments, native runtime work and training changes; local qualification must preserve those boundaries.

The [local BitNet CPU screen](validation/vibevoice-bitnet-2026-09-18.md) now records actual execution of its specialized GGUF artifacts, separate model-loading/request timings and a decoder-position correction experiment. Official resident-file/token-output support does not provide the streaming checkpoint's waveform state. The encoder arena grows with input length, and a small checkpoint file does not establish low long-recording RAM. Neither a paper speedup nor the local position correction establishes an application latency improvement.
