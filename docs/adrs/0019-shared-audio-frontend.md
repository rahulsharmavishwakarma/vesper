# ADR-0019: Share advisory VAD and optional enhancement across recording modes

Status: Accepted
Date: 18 September 2026
Tracking: [issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42)

## Context

The user approved the concrete first-stage integration after reviewing its data flow, ownership, model roles and validation plan. Earlier frontend measurements found quiet-speech VAD misses, denoiser word regressions, packet-dependent VAD boundaries and different offline/online denoiser startup. Existing ASR architecture adapters must not each acquire a duplicate audio pipeline. Ear and Uhm need additional execution runtimes and are deferred.

## Decision

Use the existing Rust inference worker and Sherpa 1.13.8 dependency. A selected `FrontendCache` owns Silero v6.2 and at most one DPDFNet4/8 handle. `FrontendSession` borrows it exclusively, resets native recurrent state at entry and Drop, and owns fresh resampling/framing/alignment/annotation state. Full hash verification precedes native creation; shared observed-file identities invalidate resident reuse after changes. Finalization checks identity again. Unload releases frontend and existing ASR/writer handles together.

Both settings default off. Silero produces advisory intervals on original canonical PCM, preserving every sample for ASR. Noise reduction is an explicit experimental choice. No probability threshold veto, silence deletion, automatic endpoint, language switch, filler removal, adaptive enhancer selection or weight training is introduced.

The existing Rubato FFT converter is stateful and shared by full-file and live frontend input. It preserves nearest-rounded length, removes filter delay and flushes the final partial frame. One recording must keep its input rate stable. Silero receives 64 initial zeros and deterministic 512-sample steps; final residual/context padding stays inside VAD. Bookkeeping flushes every 320 steps bound native active-segment copies without resetting its recurrent state. Interval coordinates subtract the context prefix and clamp to real PCM. At most 4,096 intervals are retained; additional intervals are counted, while transcription continues with all audio.

DPDFNet uses the online API in both application modes, with the pinned 16 kHz/160-hop artifacts. The host discards exactly 640 initial output samples and feeds 640 zeros only at finalization. Native cumulative length must equal real canonical length plus 640; delivered length must equal real length. Empty buffered output is valid. The offline-only attenuation setting is not used.

Inference stays outside the capture callback. Existing bounded queues and original recording retention remain. A recoverable completed frontend failure records a warning and transcribes original audio. If optional frontend initialization fails before consuming audio, live ASR starts with original input and a warning; ASR initialization errors and cancellation remain errors. An interrupted enhanced live session never resumes with raw chunks mixed into that session; its partials are invalidated and the existing completed fallback uses original audio. Both transcript views clear the failed draft; an operation-scoped invalidation marker rejects delayed partial events. Cancellation propagates, rather than becoming a fallback request. The stateless library helper reports frontend failures to its caller; the application's manager owns automatic fallback and its diagnostics.

Only existing native-streaming ASR adapters emit partials, currently Nemotron. Cohere, Whisper and LFM process the selected frontend over the completed recording at Stop. This does not add a second capture worker or a streaming decoder to completed-only models.

Reuse installer integrity/download/removal and settings controls through the `audio_frontend` model role. Overall inference readiness requires ASR and the enabled writer; missing optional frontend files remain visible in inventory with an original-audio fallback notice. Each frontend package retains its own installation readiness. DPDFNet4/8 are artifact choices within one runtime implementation. Speech intervals and stage measurements are content-free execution evidence; no model weights or personal voice data are learned.

## Narrow native correctness fix

Periodic VAD `Flush` during silence hangover exposed an existing endpoint calculation that could precede the next segment start. The pinned native patch now performs extraction/enqueue/pop only for `end > start_`. Positive intervals retain their prior behavior; invalid empty segments and range errors are removed. This is a boundary bug fix, not a new decoder, kernel, probability binding or model format. The integration regression covers pauses near the bookkeeping boundary and subsequent speech recovery.

Sherpa upstream remains `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`. The initial integration build is `67ce39e3334e098b428e0e94284c3b6a0846e014a5c08000c02ae28f393ebc7b`; patch SHA-256 `22d05a9249ede683930e3f47c9c69ebeed66d85aaa8925914dca4b310984bae3`. Exact static-library, source and executable hashes belong to each validation envelope.

## Evidence and engineering categories

- **Paper result:** [DPDFNet](https://arxiv.org/abs/2512.16420v1) establishes a causal enhancement architecture. [Ochiai et al.](https://arxiv.org/abs/2404.14860v1) show enhancement artifacts can hurt recognition. Their hardware/checkpoints/results are not this application's measured performance.
- **Official capability:** [Sherpa DPDFNet documentation](https://k2-fsa.github.io/sherpa/onnx/speech-enhancement/dpdfnet.html), [pinned VAD source](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/voice-activity-detector.cc) and [online STFT implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/online-speech-denoiser-stft-impl.h) establish the reset, buffering and flush contracts.
- **Local measurement:** [earlier regression](../validation/audio-regression-2026-09-16.md) motivated the guards and common online path. The [integration qualification](../validation/audio-frontend-integration-2026-09-18.md) records this application path separately; improvements cannot be inferred from the earlier benchmark-only runs.
- **Unverified hypothesis:** optional enhancement improves a particular user's noisy dictation. It remains experimental pending paired quality evidence.

This is frozen-weight application/runtime integration, plus the narrow native range guard. FFT/operator/decoder optimization, a scalar-VAD binding and new quantized artifacts remain separate runtime work. Training, checkpoint changes and user-voice learning remain separate work.

Artifact identities are canonical in [the production manifest](../../models/artifacts.json). Silero package `7e30209a3e901f9842f81b225f3e93d8199902b1` retains the November 2025 weight revision `bfdc0193023f121ea5b3cc7b176dbed570a68a59`. DPDFNet uses author export `3b0a4ee55df845f6afd6be7e3253cdf5dbcfe634`, matching the tested content-addressed Sherpa release files. The March 20 exports are not substituted.

## Consequences and validation

Enhancement consumes CPU and introduces buffering; advisory VAD does not reduce ASR decoder work. Original-audio retention supports recovery but retains its memory cost. Process RAM is not model download size. Current native calls are synchronous; cancellation is checked between calls, and arbitrary internal C++ exceptions are not promised recoverable.

Require packet-schedule equivalence, exact output lengths, finite PCM, onset/Stop retention, 44.1/48 kHz conversion, A/B/A reset, cancellation/drop/unload and bounded long-recording state. Compare the same frozen original/advisory/enhanced inputs through actual ASR and writing models, preserving errors and unsupported-mode exclusions. Keep full-file throughput, paced replay, physical microphone delivery and user-visible latency distinct. Opt-in correctness does not establish default-quality promotion or SOTA CPU/edge latency. See [the research and qualification contract](../research-audio-quality.md).
