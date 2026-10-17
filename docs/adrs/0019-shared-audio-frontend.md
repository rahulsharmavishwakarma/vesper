# ADR-0019: Share advisory VAD and optional enhancement across recording modes

Status: Accepted
Date: 18 September 2026
Tracking: [issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42)

## Context

The user approved the concrete first-stage integration after reviewing its data flow, ownership, model roles and validation plan. Earlier frontend measurements found quiet-speech VAD misses, denoiser word regressions, packet-dependent VAD boundaries and different offline/online denoiser startup. Existing ASR architecture adapters must not each acquire a duplicate audio pipeline. Ear and Uhm need additional execution runtimes and are deferred.

## Decision

### Approved automatic level extension (19 September 2026)

The user approved the [bounded automatic-level proposal](https://github.com/rahulsharmavishwakarma/transcribe/issues/42#issuecomment-5740777803)
with “ok complete it.” `audioFrontend.autoLevelEnabled` is an independent opt-in
setting. It reuses the shared frontend and resident Silero handle; it changes no
ASR adapter, native kernel, decoder, checkpoint or model ownership.

The experimental `bounded-level-v1` controller uses deterministic 512-sample
(32 ms) frames in both recording modes. A separate continuous gain envelope
conditions only the detector's input (target −26 dBFS, maximum 60 dB). This is
not a noise classifier. The detector's public `detected()` value is hysteretic,
not a probability; it is read before bookkeeping flush. Original samples feed
the main level/noise estimates. Main gain begins at unity, targets −32 dBFS,
and is limited to 30 dB, estimated noise and current-frame 0.95 peak headroom.
The −50 dBFS noise target limits additional amplification; it does not reduce
already-louder noise to that level. The frame-endpoint gain budget is 30 dB/s
up and 120 dB/s down, with amplitude interpolation inside the frame and
immediate reduction when headroom demands it. A held speech-level estimate
with 6 dB/s release limits chasing abrupt falling tails; gradual level declines
can still permit gain increases. Initial non-speech confirmation is not
immediately treated as noise. These constants are engineering hypotheses.

Frame buffering adds up to 32 ms before processing (in addition to resampling,
VAD response, denoising and ASR latency). No utterance lookahead or silence
deletion is used. A final partial frame contributes only real samples to gain
statistics and output; detector-only padding stays internal. Session reset
clears both envelopes and noise/speech estimates. Advisory intervals, when
requested together with automatic level, describe conditioned detector input;
the off mode retains original-PCM annotations. Post-denoiser peaks are measured
separately, because safe input peaks do not guarantee safe enhanced output.
`firstSpeechSample` and `firstGainSample` identify the affected frame's start;
they are not decision/emission timestamps. Decisions become available at frame
end, up to 32 ms later. A startup false detection can permit gain before a noise
estimate exists, so the noise ceiling is an estimate-based constraint, not a
guarantee that arbitrary background sounds will remain unamplified.

This is frozen-weight application DSP supported by Sherpa 1.13.8's public API.
[Pinned WebRTC AGC2](https://webrtc.googlesource.com/src/+/b32e4b35e6025f044e88f4cc3169523b3021e3a9/modules/audio_processing/agc2/adaptive_digital_gain_controller.cc)
provides design references for speech/noise constraints and gain limits; its
post-suppression placement and constants are not copied as validated settings.
The [artifact research](https://arxiv.org/abs/2404.14860v1) and the local
[matched-level diagnostic](../validation/denoising-followup-2026-09-19.md#gain-matched-diagnostic-results)
motivate qualification, not a guarantee of automatic-policy accuracy. Runtime
and artifact revisions below remain unchanged. Default promotion, automatic
denoiser switching, endpointing and training remain outside this extension.

### Original optional frontend decision

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

## Amendment, 17 October 2026: one live session per recording

Approved by the owner in [#141](https://github.com/rahulsharmavishwakarma/vesper/issues/141).
Buffered previews now exist for completed-only models, and the desktop always
enables Silero and DPDFNet8. Re-running the frontend over the completed recording
at Stop therefore repeated work already done during recording. DPDFNet8 costs
about 0.19× the recording length on one CPU thread (20.1 s for a 108 s recording
on an Apple M5 Pro), and more ONNX Runtime threads did not reduce it.

Live recordings now run one continuous frontend session. Its enhanced recording is
reused by the completed-audio final when operation, settings and length match.
The completed-recording path above still applies to imported files, to a
mismatch, and to any recording without a retained session. A failed live session
uses the original recording with a warning instead of a second frontend pass.
Measurements and parity evidence are in [live transcription](../live-transcription.md#live-frontend-reuse-evidence).
