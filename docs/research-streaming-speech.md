# Streaming speech: NVIDIA, Kyutai STT and Moshi

Current selection update (18 September 2026): [GitHub issue #48](https://github.com/rahulsharmavishwakarma/vesper/issues/48) records the removal of Kyutai from the application catalog, installer, and runtime after its local US-English CPU screen failed the declared quality and real-time gates. Kyutai and Moshi material below is retained as research history, not current application support. Consult [application internals](internals.md) for current capabilities.

Research updated **14 September 2026**; product status updated 18 September 2026. The cross-model paper-to-engineering analysis is in [Research basis for streaming inference](research-streaming-inference-papers.md). Nemotron is the selectable multilingual/live adapter through pinned NeMo-Speech.cpp 0.1.0, with resident weights, fresh per-recording streams, partial/final events, and completed-audio inference. Cohere is the fresh-settings default for completed recordings.

**Historical recommendation and outcome:** the proposed Nemotron migration from per-job file inference to NVIDIA's persistent C ABI is implemented for multilingual live dictation, including Hindi. Parakeet remains an offline compatibility baseline. Kyutai was evaluated through its application adapter and then removed; Moshi fits a later spoken conversational assistant, not the user's auditable transcript.

## Verified model shortlist

| Model / exact repository | Task and scale | Language scope | Streaming behavior | Published weight license |
| --- | --- | --- | --- | --- |
| [nvidia/nemotron-3.5-asr-streaming-0.6b](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b) | Prompt-conditioned ASR, 600M | 32 locales usable out of the box, including Hindi; 8 further locales require adaptation | Cache-aware RNNT; advertised chunk choices 80/160/320/560/1120 ms | OpenMDW-1.1 |
| [nvidia/nemotron-speech-streaming-en-0.6b](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b) | ASR, 600M | English | Cache-aware RNNT; 80/160/560/1120 ms configurations | NVIDIA Open Model License |
| [nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) | ASR, 600M | 25 European languages; no Hindi | Full-utterance TDT; our existing adapter is offline | CC-BY-4.0 |
| [kyutai/stt-1b-en_fr](https://huggingface.co/kyutai/stt-1b-en_fr) | ASR, about 1B | English and French | Delayed text stream, 0.5 s offset; semantic end-of-turn prediction | CC-BY-4.0 |
| [kyutai/stt-2.6b-en](https://huggingface.co/kyutai/stt-2.6b-en) | ASR, about 2.6B | English | Delayed text stream, 2.5 s offset | CC-BY-4.0 |
| [Kyutai Moshi / Moshiko / Moshika](https://github.com/kyutai-labs/moshi) | Full-duplex speech conversation, 7B temporal transformer plus audio components | Released examples/checkpoints here are English | Simultaneous listening and generated speech; reported 200 ms on NVIDIA L4 | CC-BY-4.0 |

The exact checkpoint matters. NVIDIA's English streaming repository now identifies a March 2026 checkpoint and retains its January checkpoint under a separate revision. A generic label such as “Parakeet streaming” does not distinguish a cache-aware RNNT, offline TDT, buffered CTC or an end-of-utterance model. Pin the selected model and runtime commits when running the proof. [English checkpoint history in the model card](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b#release-date), [native ASR model types](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/asr/models.md).

The Nemotron 3.5 card's 40-locale headline includes 19 transcription-ready, 13 broad-coverage and 8 adaptation-ready locales. Hindi `hi-IN` is transcription-ready. Automatic language tagging describes mixed-language traffic; it is not evidence of accurate Hindi/English switching within a sentence. Treat Hinglish, English names in Hindi, script choice, numbers and technical vocabulary as separate evaluation cases. [Current language tiers](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b#supported-languages).

## Moshi and Kyutai STT solve different problems

Moshi models the user's speech and the assistant's speech as parallel streams. Its text stream primarily accompanies the assistant's own generated speech. Full duplex allows interruptions and overlap without waiting for a conventional turn boundary. That is useful for conversation, but it adds generation and audio-output behavior that a dictation utility does not need. Preserve a separate ASR path if a future assistant also needs an auditable record of what the user said. [Moshi paper](https://arxiv.org/abs/2410.00037).

Kyutai STT uses the related **Delayed Streams Modeling** formulation specifically to predict text from audio. Delaying text gives the decoder some later acoustic context before emitting a word. The public STT models produce punctuation and recoverable timestamps; they are not just Moshi dialogue prompts repurposed as dictation. [DSM paper](https://arxiv.org/abs/2509.08753), [Kyutai STT model architecture](https://huggingface.co/kyutai/stt-1b-en_fr#model-details).

For our product, semantic end-of-turn prediction is potentially useful when someone pauses in the middle of a sentence. It should remain advisory: a pause or predicted completion must not discard the remaining audio or force an irreversible rewrite. Kyutai's project exposes semantic VAD through its Rust implementation, and its standalone Rust example also exposes VAD output. [Kyutai semantic VAD](https://kyutai.org/stt/#semantic-voice-activity-detector), [Rust example](https://github.com/kyutai-labs/delayed-streams-modeling/blob/main/stt-rs/src/main.rs).

## Native integration: Python is optional for inference

### NVIDIA now has an official native speech runtime

[NVIDIA/NeMo-Speech.cpp](https://github.com/NVIDIA/NeMo-Speech.cpp) is an official C++/ggml inference project supporting Nemotron 3.5, English Nemotron, Parakeet TDT and Parakeet CTC. NVIDIA-authored runtime code is Apache-2.0; model licenses remain separate. This changes the earlier assumption that evaluating multilingual Nemotron necessarily requires shipping a Python service.

Its build documentation lists CPU, CUDA, Metal and Vulkan configurations. The ASR-only build can avoid unrelated TTS/translation components. A platform build option is evidence of an available route, not proof of useful speed or successful packaging on this machine. Start with CPU and Metal on the available Apple M5 Pro; qualify CUDA separately on a named NVIDIA system. [Native build requirements and presets](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/build.md).

The supported native interface is a **C ABI with opaque handles**, appropriate for a small Rust binding. It has model creation, streaming audio push, result polling, explicit finish and destruction, with documented ownership and single-thread-per-stream rules. A persistent worker could hold the recognizer across recordings while creating fresh stream state. The SDK consumes mono float32 audio and can resample input rates from 8–96 kHz. [SDK ownership and streaming flow](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/sdk.md).

The header exposes `nemo_speech_asr_result_is_final`, an immediate endpoint request, and a finish operation that flushes the tail. These are the controls needed to distinguish provisional captions from committed utterances. Rust should copy result text before destroying its native result handle. [Public ASR header](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/include/nemo_speech/asr.h).

A local sidecar is another proof route: the upstream server has an explicit WebSocket transcription protocol with partial and completed events. It is separate from its conversational VoiceChat protocol. A sidecar lets us evaluate failures and cancellation before introducing an FFI boundary; an eventual in-process adapter should isolate unsafe FFI in a reviewed binding crate rather than relaxing safety across application code. [Transcription protocol](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/api.md#websocket-v1audiotranscriptionsrealtime).

**A speech GGUF is not a cleanup GGUF.** Nemotron's GGUF belongs to NeMo-Speech.cpp's speech architecture. Our `llama_cpp` profile loads text instruction models with chat templates and cannot run it. A new explicit speech adapter/profile type is required. The current `sherpa_onnx_transducer` profile also does not implement Nemotron's cache management or language conditioning. The current Transformers configuration additionally declares 128 mel bins, so copying the existing feature assumptions would be unsafe. [Nemotron configuration](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/blob/main/config.json), [current profile contract](huggingface-models.md).

### Kyutai has a direct Rust implementation too

Kyutai provides a Rust server, a standalone Rust/Candle STT example, Python/PyTorch examples and MLX paths. The standalone crate exposes CPU, CUDA and Metal options; it does not require a Python inference process. Its implementation loads a text model, Mimi codec, tokenizer and configuration, processes 24 kHz audio, and keeps DSM state. This requires a dedicated adapter, not our existing ONNX loader. [Official implementation overview](https://github.com/kyutai-labs/delayed-streams-modeling), [Rust dependencies/features](https://github.com/kyutai-labs/delayed-streams-modeling/blob/main/stt-rs/Cargo.toml), [stateful Rust implementation](https://github.com/kyutai-labs/delayed-streams-modeling/blob/main/stt-rs/src/main.rs).

The published MLX commands use Python, but the same project links a Swift implementation for Apple devices. That is a possible Apple-specific route, not a reason to add Python to every installation. Moshi itself also has Rust/Candle and MLX implementations. Python remains useful for reference experiments, conversions and later fine-tuning; it need not be the application runtime. [Kyutai MLX/Swift guidance](https://github.com/kyutai-labs/delayed-streams-modeling#kyutai-speech-to-text), [Moshi backends](https://github.com/kyutai-labs/moshi#organisation-of-the-repository).

## Artifact sizes are not memory requirements

These are rounded sizes currently shown by the official repositories, not measured peak RAM/VRAM:

| Candidate representation | Published files | Implication for initial evaluation |
| --- | --- | --- |
| Nemotron 3.5 Q8 GGUF | About 742 MB, versus about 2.55 GB for the published F32 safetensors | Small enough to evaluate before adding multi-gigabyte alternatives; runtime buffers still need measurement. [Files](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/tree/main) |
| Kyutai STT 1B Candle | About 1.98 GB text-model weights plus 385 MB Mimi | Budget for both model components and streaming state. [Files](https://huggingface.co/kyutai/stt-1b-en_fr-candle/tree/main) |
| Kyutai STT 2.6B | About 5.23 GB model plus 385 MB Mimi | A larger comparator; its longer output delay already weakens the fit for immediate dictation. [Files](https://huggingface.co/kyutai/stt-2.6b-en/tree/main) |
| Moshi Moshiko MLX Q4 / Candle Q8 | About 4.81 GB / 8.17 GB model files, plus 385 MB codec | Consider only for a distinct conversation feature. [MLX files](https://huggingface.co/kyutai/moshiko-mlx-q4/tree/main), [Candle files](https://huggingface.co/kyutai/moshiko-candle-q8/tree/main) |

Measure loaded weights, codec/feature buffers, encoder caches, decoder state, compute allocations, and simultaneous cleanup residency. Unified memory does not make these free. The Moshi README's unquantized PyTorch guidance mentions a 24 GB GPU; that must not be generalized to the smaller STT models or quantized Apple paths. [Moshi backend requirements](https://github.com/kyutai-labs/moshi#requirements), [application performance protocol](inference-performance.md).

## Streaming latency, past context and future context

A streaming model can use audio already heard, and it can delay emission to collect some later audio. It cannot know a future correction the speaker has not supplied. Cached past context reduces redundant computation; it does not remove the latency/accuracy tradeoff from lookahead. This is the motivation for cache-aware Conformer inference. [Stateful Conformer paper](https://arxiv.org/abs/2312.17279).

| Quantity | Meaning | Why it is not final-text latency |
| --- | --- | --- |
| Audio/codec frame | Smallest processing time step | Several frames may be needed before a useful text token. |
| Lookahead or text delay | Audio the model waits for before deciding | Scheduling and inference happen in addition. |
| First partial | First provisional caption shown | Later hypotheses or additional words can change interpretation. |
| Endpoint delay | Time used to decide an utterance ended | A user can pause and then continue or correct themselves. |
| Stop-to-final ASR | Explicit stop until tail is decoded and finalized | Cleanup and persistence may still remain. |
| Stop-to-accepted text | Final ASR plus cleanup/guard/UI delivery | This is the main user-facing dictation metric. |

Nemotron 3.5's documented NeMo configuration uses 80 ms encoder units, with `[56,R]` and chunk duration `(R+1) × 80 ms`. The 56-frame past window is context, not a 4.48-second wait before each result. However, the currently published Transformers config lists lookahead choices `3,0,6,13`, omitting the card's 160 ms operating point. Verify the selected runtime and artifact's supported values instead of assuming all interfaces expose identical choices. [Model card configuration](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b#setting-up-streaming-configuration), [Transformers config](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/blob/main/config.json).

The native runtime distinguishes CTC buffered-window settings from RNNT right-context settings. Its direct library use disables batching by default; server defaults can add batching queues. Endpointing is another independent setting, disabled by default, with an 800 ms trailing-silence default when enabled. Do not map a single “latency” slider to all these unrelated quantities. [Native ASR configuration](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/asr/configuration.md).

Kyutai's 0.5 s and 2.5 s offsets are deliberate model delays. Its published “flush trick” example assumes a server processing about four times real time, turning a remaining 500 ms model delay into roughly 125 ms of computation. That rate is a deployment observation, not a Mac guarantee. Its 400-stream H100 statement measures concurrency, not one user's stop-to-cleaned-text time. [Kyutai delay and throughput explanation](https://kyutai.org/stt/#low-latency).

Likewise, Moshi's 160 ms theoretical / about 200 ms L4 figure describes a speech-conversation pipeline. NVIDIA's Nemotron concurrency plots use a single H100 and median final-token latency at each concurrency. These figures cannot rank desktop dictation responsiveness against each other. [Moshi architecture and latency](https://github.com/kyutai-labs/moshi#model-architecture), [Nemotron benchmark conditions](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b#throughput--efficiency).

## Product behavior to preserve while adding partial captions

The following is a proposed application design, not existing functionality:

1. Keep one recording/session identity and immutable original audio. Feed bounded PCM chunks to a persistent ASR worker while archiving the source independently.
2. Emit structured events with session ID, utterance ID, sequence number, consumed audio time, partial text, and final status. Replace the active provisional span rather than appending overlapping hypotheses.
3. Preserve each ASR final as a prediction. On explicit Stop, flush the resampler and ASR tail, drain results, and only then finalize the session. Natural pauses may create utterance boundaries without closing the recording.
4. Run cleanup after finalization initially. An optional later sentence-level preview must be replaceable and must not overwrite raw text or previously human-approved labels.
5. Treat corrections such as “send it Friday… no, Monday” using the actual continuation. Never complete unfinished thoughts because a timer expired. Keep raw and cleaned variants linked, including any rejected cleanup.
6. Record language prompts/tags, endpoint settings, context policy, timestamps, adapter/build, model revision and hashes. Keep word boosting, inverse text normalization, punctuation postprocessing and LLM cleanup distinguishable in provenance.

For the native proof, disable optional profanity filtering and inverse text normalization so that later formatting is not silently folded into the raw transcript. Native ASR may already emit punctuation; “raw” means the saved ASR prediction, not a claim of human-verified verbatim ground truth. The upstream runtime documents independently gated final-result postprocessing. [Postprocessing controls](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/asr/configuration.md#postprocessing-profanity-itn-pnc).

## Other NVIDIA candidates

**Parakeet Realtime EOU 120M** is a useful small English comparator: `nvidia/parakeet_realtime_eou_120m-v1` emits an end-of-utterance token, advertises 80–160 ms streaming behavior, and omits punctuation/capitalization. It has a different model license and still needs a qualified runtime path; being smaller does not make its artifacts compatible with the app. [Official model card](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1).

**Canary 1B v2** covers transcription and translation for 25 European languages, without Hindi. **Canary-Qwen 2.5B** is English ASR plus a separate text-LLM mode; its card describes 40-second training audio and a 1,024-token training sequence limit. Neither is the first choice for our small multilingual streaming adapter. They are sensible later offline accuracy comparators, and both list CC-BY-4.0. [Canary v2](https://huggingface.co/nvidia/canary-1b-v2), [Canary-Qwen architecture and limits](https://huggingface.co/nvidia/canary-qwen-2.5b).

**Multitalker Parakeet** targets overlapping speakers using external diarization and an ASR instance per speaker. That adds state and compute for meeting transcription; it is unnecessary for the initial single-user dictation path. [Official multitalker model](https://huggingface.co/nvidia/multitalker-parakeet-streaming-0.6b-v1).

## Historical execution proof and decision gates

This was the execution plan before Kyutai's removal. Nemotron now exposes live and completed-audio inference, and Cohere is the current fresh-settings default. The Kyutai 1B CPU lane was executed on 18 September; it completed all 16 samples but measured 29.63% micro-WER and 2.250 median RTF, failing the screen's 20% WER and 1.0 RTF ceilings. The adapter and artifacts were therefore removed under issue #48. The remaining NVIDIA qualification work is:

1. **Freeze evaluation inputs.** Use consented/human-referenced English, Hindi and code-switched clips plus silence, noise, pauses, self-corrections and a word ending exactly at Stop. Keep existing Parakeet results as the English baseline.
2. **Prove every packaged device path.** Run whole-file and recorded-streaming modes on CPU and Metal, then named CUDA and Vulkan systems. Record actual backend logs, quality, latency, and peak memory. The upstream guide exposes recorded streaming with `--stream`. [Supported native modes](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/asr/models.md).
3. **Replay at wall-clock pace.** Feed the same recording incrementally, test supported right-context settings, and log first partial, partial changes, endpoint decisions, final ASR and stop-to-accepted-cleanup latency. File decoding faster than real time alone is not a live-streaming proof.
4. **Exercise lifecycle boundaries.** Test several consecutive streams on one resident recognizer, explicit Stop, cancel, end-of-stream tail flush, device interruption, long silence, queue backpressure, window close and model switch while idle. Prove that no previous utterance leaks into the next stream and archived audio survives recoverable errors.
5. **Keep removed alternatives evidence-gated.** Any future Kyutai reconsideration starts as a new proposal with a pinned adapter and must pass representative English/French, latency, memory, and device gates before returning to the catalog. Use the [measurement protocol](inference-performance.md#reproducible-benchmark-protocol).

The decision gate is accurate final text, preserved audio/provenance, bounded memory and reliable cancellation on the target device. Upstream feature lists establish feasibility; this proof establishes whether the candidate belongs in the application.
