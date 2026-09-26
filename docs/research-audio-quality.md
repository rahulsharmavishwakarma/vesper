# Audio frontends for dictation quality

Research date: 16 September 2026. The user allows **up to 200 MB per candidate**, welcomes smaller models when they perform well, and now requires **recent model releases from 16 September 2025 onward**. This supersedes the earlier sub-2-MB proposal and excludes older checkpoints from the shortlist. We researched 38 initial inventory entries plus Ear/Uhm and audited additional recent releases; older results remain baselines, not recommendations. This is a non-exhaustive search, not 38 executed models. [Issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42) owns the proposal and approval status.

The development benchmark executes **Silero v6.2**, DPDFNet-4/8 and isolated CPU Uhm/Ear probes, with old Silero v4 FP32/INT8, GTCRN and CED-mini/base as baselines. The [hard regression results](validation/audio-regression-2026-09-16.md) extend the [initial measurements](validation/audio-frontends-2026-09-16.md) with previously unused recordings, broader corruptions, AMI annotations and longer replay. See also the [candidate inventory](validation/audio-frontend-candidates.json), [release audit](validation/audio-frontend-recency.json) and [download hashes](../benchmarks/asr/audio-frontend-artifacts.json). The earlier [source audit](validation/audio-quality-research.json) remains historical evidence. Model payload size is not process RAM or latency. The approved [shared production frontend](adrs/0019-shared-audio-frontend.md) integrates Silero and DPDFNet. As of 26 September, the owner selected a fixed desktop setup: Silero advisory timing and DPDFNet8, with automatic downloads and no audio model picker. Automatic gain remains disabled; benchmark defaults remain raw. This product choice does not clear the existing ASR-quality gates. See [current setup and limitations](user-guide.md#experimental-audio-processing). Ear/Uhm remain benchmark-only.

## Nemotron 3 Diarization general release — 25 September 2026

The [general-release model](https://huggingface.co/nvidia/Nemotron-3-Diarization)
is dated 23 September 2026. It supersedes the preview-only exclusion **for this
new artifact**, not for the separately named preview weights. NVIDIA describes
commercial/noncommercial use under [OpenMDW 1.1](https://openmdw.ai/license/1-1/);
redistribution must retain that license and applicable origin notices. The
release now has an experimental completed-recording integration under
[ADR-0022](adrs/0022-local-speaker-timelines.md). The [26 September application
follow-up](validation/nemotron-diarization-2026-09-25.md#automatic-policy-validation--26-september-2026)
removes the original default-off switch: analysis now runs automatically when its
local components and backend are available. This requested behavior change and local
execution do not qualify general speaker accuracy or improved ASR word accuracy.

The official repository revision is
`f667ed73aee57d40cc39428eb768b4fd87a0a29e`. Its Q8_0 GGUF is **107,012,128 bytes**
(about 107 MB), within the existing 200 MB candidate limit. Publisher SHA-256:
`08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1`.
The NeMo archive is 198,676,480 bytes; the Transformers weights are 396,954,592
bytes and exceed that limit. These are
[published artifact metadata](https://huggingface.co/api/models/nvidia/Nemotron-3-Diarization?blobs=true).
The subsequent [local sample test](validation/nemotron-diarization-2026-09-25.md)
downloaded and verified the Q8_0 hash and measured process memory separately;
the other formats were not downloaded.

The approximately 100M-parameter model emits anonymous activity for up to eight
speakers at 10 ms resolution. Recommended streaming input buffers are 0.32,
0.64 and 1.04 seconds; 30.4 seconds is an offline-style chunk configuration.
Those numbers exclude compute and are not stop-to-final or stable-speaker
latency. Speaker order within a recording does not establish the owner's
identity. The model card's GPU performance is not a Mac measurement.

### Native feasibility and transcription-quality boundary

[NeMo-Speech.cpp source revision](https://github.com/NVIDIA/NeMo-Speech.cpp/tree/97a15afa5caa9bce5baaa86c1184103877af4101)
`97a15afa5caa9bce5baaa86c1184103877af4101` provides standalone diarization and
word-level speaker tags, with CPU and Apple Metal build paths. The
[CLI contract](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/97a15afa5caa9bce5baaa86c1184103877af4101/docs/cli.md)
accepts local GGUF paths without network use. Source availability is not proof
that this application's pinned v0.1.0 archive supports the new checkpoint or
that Metal inference has been qualified.

The inspected [native defaults](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/97a15afa5caa9bce5baaa86c1184103877af4101/docs/asr/models.md#sortformer-speaker-diarization)
use 13 chunk frames plus one right-context frame: **1.12 seconds of buffering**
before compute. Do not substitute the model card's 0.32-second configuration
for that native default. The [build workflow](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/97a15afa5caa9bce5baaa86c1184103877af4101/.github/workflows/build.yml)
describes Metal compilation/linking with CPU inference coverage and a
non-blocking Metal probe, not a qualified Metal performance result.

Two different integration paths must remain separate:

- The native ASR path supplies the same audio to recognition and diarization,
  then attaches speaker tags to recognized words. Adding these tags does not
  establish lower word error rates or recovery of overlapping words. See the
  [recognizer](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/97a15afa5caa9bce5baaa86c1184103877af4101/src/asr/recognizer.cpp)
  and [word attribution](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/97a15afa5caa9bce5baaa86c1184103877af4101/src/asr/diar/diar_pipeline.cpp).
- NVIDIA's [NeMo ASR integration guide](https://huggingface.co/nvidia/Nemotron-3-Diarization/blob/f667ed73aee57d40cc39428eb768b4fd87a0a29e/ASR_INTEGRATION_GUIDE.md)
  uses separate ASR streams per speaker: Multitalker Parakeet with speaker
  conditioning, or Nemotron 3.5 with speaker-activity masking. This is a different
  pipeline from native word tagging, with additional runtime/state requirements.
  It does not provide an interchangeable mask for the current Cohere adapter.

Historical code inspection at main `59c6d21` found the missing wiring:
`transcribe-models` pins runtime 0.1.0; `transcribe-nemo` sets recognizer `diar`
to null, never enables request diarization, and exposes text/finality/duration
without speaker segments or word timestamps. The initial release review did
not run weights; the follow-up below supplied standalone local evidence. The
subsequently approved integration adds a separate namespaced standalone C ABI
bundle, bounded worker request, validated overlapping timelines, session/export
persistence and latest-result/History display. Existing ASR ownership and word
output remain unchanged. See the [integration validation](validation/nemotron-diarization-2026-09-25.md#application-integration-validation).

### Local sample evidence and the release article

NVIDIA's [23 September release article](https://huggingface.co/blog/nvidia/nemotron-diarization)
explicitly separates diarization from speaker-attributed ASR, input buffering
from end-to-end latency, and anonymous channels from identity verification.
Its NVIDIA GPU throughput and published DER do not describe this Mac build.
On-device support in another vendor's SDK is not evidence of integration here.

The [25 September smoke test](validation/nemotron-diarization-2026-09-25.md) ran
two annotated meeting windows, an English single-speaker clip and silence on
CPU and Metal, plus a meeting confirmation on each. All ten native processes
completed. Both backends detected the expected 4/4/1/0 speakers. Metal took
1.057 s for 45 s of audio and .594 s for 30 s, including per-process loading;
four-sample mean .696 s. These are completed-file measurements, not live latency.

The strict boundary check failed on the 45-second clip in both original and
confirmation runs: the last segment ends 10 ms after the input. The 30-second
window's coarse segment-reference DER was 18.44% CPU / 18.47% Metal; missed
overlap/activity remains despite the correct speaker count. This establishes
local execution feasibility and a native timestamp contract defect. The app
adapter now clips only the measured 10 ms tail padding, records the adjustment
and rejects larger violations; the raw CLI failures remain in the report.
It does not establish lower WER, owner identification, accent robustness,
production readiness or permission to promote a default.

### Evidence and next evaluation

The [Streaming Sortformer paper](https://arxiv.org/abs/2507.18446v1) supplies
the speaker-cache technique, not the new checkpoint's application results.
Official model/native support above establishes an experiment path. Better
multi-speaker transcription, fewer background-speaker insertions and acceptable
Mac latency remain unverified hypotheses.

A bounded evaluation should pin the GGUF and native build independently, use
local public/synthetic audio with reference speaker times, and retain all failed
requests. Compare single-speaker controls, turn changes, overlapping voices,
quiet/noisy speech and background television. Report diarization error and
speaker-attributed WER separately from ordinary WER, plus first/stable speaker
delay, stop-to-final overhead, CPU/Metal provider evidence, peak memory and
reset/cancellation isolation. Existing ASR manifests remain the transcription
regression controls; a speaker-labelled corpus is also required.

Public-API frozen-weight work covers a standalone benchmark, fresh per-recording
state and result alignment. Runtime upgrades/ABI adaptation or per-speaker
decoding need separate implementation qualification. Personal speaker training
and new checkpoints remain training work. Production ownership, transcript/UI
data flow were subsequently approved only for the bounded timeline stage in
ADR-0022. Automatic audio filtering, speaker-conditioned ASR and personal
identity remain outside that approval under #42/#35.

## Production integration status — updated 26 September 2026

Silero v6.2 and DPDFNet4/8 share a production stage and installer entries. The desktop now fixes its audio selection to Silero and DPDFNet8 and downloads supported missing packages automatically after the required speech/writer models are ready. Speech activity remains advisory and does not delete input; enhancement remains experimental, with mixed ASR results. DPDFNet4 remains an evaluation comparator but is no longer a desktop choice. `crates/transcribe-inference/src/audio_frontend.rs` is the shared native implementation. Ear/Uhm still use isolated evaluation probes and are not production stages. See [ADR-0019](adrs/0019-shared-audio-frontend.md) for the accepted design and exact native build boundary, and the [application-path qualification](validation/audio-frontend-integration-2026-09-18.md) for paired quality results and measurement limits.

The 19 September automatic-level extension added an independent, default-off setting
using the same frontend and Silero handle. The current managed desktop profile
disables it; explicit evaluator profiles can still test it. It combines a conditioned detector
input with original-PCM level/noise estimates and bounded gain before the selected
denoiser. This does not identify the user's voice or classify arbitrary sounds.
Its thresholds and conditioned-VAD behavior are experimental engineering choices;
the earlier matched-RMS diagnostic does not qualify them. The implementation and
validation contract are recorded in [ADR-0019](adrs/0019-shared-audio-frontend.md#approved-automatic-level-extension-19-september-2026).

The [automatic-level qualification](validation/denoising-followup-2026-09-19.md#automatic-level-implementation-and-qualification)
records 468 completed-audio attempts on three recognizers and six profiles.
Standalone gain improves Cohere's artificially quiet WER from 82% to 63%, leaves
Small at 58%, and worsens Turbo from 7% to 17% on six fresh English recordings.
Noise hallucinations and three ASR limit failures remain. This supports an
experimental user choice, with no default promotion or general accuracy claim.

The [Whisper adapter regression](validation/whisper-adapter-2026-09-18.md) did not run a separate small VAD or denoiser. Its no-speech confidence is produced by Whisper itself. Consequently, the observed Whisper noise failures are not measurements of Whisper combined with Silero or DPDFNet. The subsequent [paired frontend qualification](validation/audio-frontend-integration-2026-09-18.md) now measures those combinations: some noisy-speech WER improves, quiet speech regresses, and noise false transcripts persist. It does not qualify VAD rejection of quiet speech or promote Whisper into the picker. Ear's language identification and Uhm's filler annotations are different roles from noise suppression.

The harder frontend benchmark did not qualify automatic filtering: denoisers did not consistently improve word accuracy, Silero missed some extremely attenuated speech, and Ear/Uhm retained routing or false-positive failures. The user subsequently approved opt-in integration under issue #42; this does not promote automatic filtering. Selected Cohere/Whisper resident ownership remains shared under ADR-0017/0018.

### Why required denoising still needs ASR qualification

Denoising remains a required product capability. The experimental default describes the current qualification of these artifacts/settings, not cancellation of that requirement. A single-microphone enhancer estimates speech from a mixture and can suppress target speech or introduce artifacts while reducing noise. [Ochiai et al.](https://arxiv.org/abs/2404.14860v1) isolate the detrimental effect of enhancement artifacts on ASR in their experiments; the [DPDFNet paper](https://arxiv.org/html/2512.16420v1) explicitly addresses over-attenuation with a training loss. That loss is a checkpoint-training technique, not a switch for the downloaded weights.

Locally, DPDF8 improves noisy Whisper Small WER from 17.42% to 13.35%, while DPDF4 worsens attenuated Whisper Turbo from 6.53% to 53.27% (24 paired clips in each comparison). These are different ASR/condition/denoiser combinations, not a head-to-head denoiser ranking. The latter inputs were multiplied by 0.03; they are not natural whispers. Exact length/reset tests do not establish waveform fidelity. Follow-up diagnostics establish substantial signal-level sensitivity, while residual attenuation, waveform distortion and export/runtime contributions remain incompletely separated. Existing original-audio fallback handles processing failures; it does not detect acoustically plausible but incorrect words.

The completed [19 September follow-up](validation/denoising-followup-2026-09-19.md) records 2,153 ASR attempts using fixed 50/50 observation-adding candidates, known-gain diagnostics and paced streaming. Mixing reduces several full-suppression regressions, but introduces Cohere noise-only false transcripts and still worsens some quiet speech; neither mixture is promoted. Boosting before DPDF4 and restoring the quiet level reduces observed Turbo WER from 53.27% to 6.53%, while Cohere also needs adequate recognizer input level. These controlled gains are diagnostics, not an implemented AGC policy. Packet/reset/integrity checks pass; all inference processes were contended, and target-device latency is unqualified. The subsequent matched-level diagnostic separates one global energy difference; causal level handling and fresh microphone-like quiet/noisy and noise-only tests remain necessary. The [Sherpa attenuation limiter](https://k2-fsa.github.io/sherpa/onnx/speech-enhancement/dpdfnet.html#offline-attenuation-limit) remains offline-only; no production default or adaptive routing changed.

The [matched-level extension](validation/denoising-followup-2026-09-19.md#gain-matched-diagnostic-results) records another 720 main attempts and 24 confirmation attempts. At equal restored RMS, DPDF4 input conditioning changes Cohere / Small / Turbo WER from 22.93 / 12.34 / 6.24% to 5.81 / 7.84 / 4.93%, versus raw controls of 5.81 / 7.98 / 5.37%. This confirms useful recovery of denoising regressions on the observed artificially quiet corpus; every restored pre-gain-versus-raw confidence interval still spans zero. All 72 unchanged baseline outcomes reproduce. A shared-worktree source guard event is preserved and its affected comparison repeats exactly from frozen files. Future audio, a known artificial gain, no fresh noise/microphone coverage and mostly contended timing prevent this diagnostic from qualifying automatic normalization. The subsequent causal controller is shared across ASR architectures and has its own mixed results above.

## First production integration contract — 18 September 2026

The user approved this concrete first-stage design. [ADR-0019](adrs/0019-shared-audio-frontend.md) describes the implementation, including the native endpoint range fix discovered during integration. [Issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42) owns approval and qualification status. The following records the scope and predeclared test gates; targets are not benchmark results.

### Scope and model roles

Implement one small, architecture-independent frontend inside the existing inference worker. Use the existing Sherpa dependency for Silero v6.2 speech-activity annotations and optional DPDFNet4/8 enhancement. Both DPDFNet variants use one implementation with different pinned artifacts. Avoid a second ASR adapter registry, Python subprocesses, or a new service.

| Stage | Proposed behavior | Qualification boundary |
| --- | --- | --- |
| Silero v6.2 | Analyze original 16 kHz PCM and report speech intervals. Preserve every input sample for ASR. | Advisory only: the measured quiet-speech misses prohibit using VAD as a transcription veto or silence-skipping policy. |
| DPDFNet4/8 | An explicitly enabled noise-reduction setting supplies aligned enhanced PCM to ASR. The same online algorithm handles paced live input and accelerated completed input. | Experimental and off by default. Neither variant has established an overall transcription-quality improvement. Select one variant per recording; do not switch on individual frames. |
| Ear | Retain the existing language-hint evaluation while choosing and qualifying a production runtime separately. | Deferred from this first implementation. It requires LiteRT or a Core ML bridge absent from the current Rust runtime. Noise and noisy-Hindi routing failures prohibit automatic model/language switching. |
| Uhm | Retain filler-span evaluation; any later integration produces reviewable annotations. | Deferred from this first implementation. The fixed 30-second noncausal ONNX graph needs a generic tensor runtime or a separately qualified Core ML bridge. It is not a causal streaming noise filter. No automatic audio or word deletion. |

Ear/Uhm deferral is deliberate, not a claim that the request is fully delivered. Their additional execution bridges, window scheduling and annotation interfaces need their own concrete scope. A macOS Core ML bridge could serve both, but the existing LiteRT/ONNX CPU measurements cannot qualify those artifacts or that backend. Uhm's measured approximately 1.84-second call and 1.47 GB whole-process peak RSS also preclude treating its 47 MB download as a negligible live-stage cost. The pinned SDK's Uhm catalog revision differs from the benchmark revision; importing that SDK wholesale would silently change the evaluated artifact and duplicate application download/lifecycle responsibilities. [Pinned SDK package](https://github.com/Desert-Ant-Labs/desert-ant-core/blob/2aed9c6d6459fc1843ab59553965035954e40199/Package.swift), [Uhm catalog](https://github.com/Desert-Ant-Labs/desert-ant-core/blob/2aed9c6d6459fc1843ab59553965035954e40199/Sources/Uhm/Catalog.swift).

### Shared PCM and session contract

1. Preserve the existing original recording and keep inference outside the audio callback. Extract the existing Rubato conversion into a request-scoped stateful resampler, reused by completed conversion. Mix once, keep one 16 kHz sample clock, and retain the current output-delay compensation and rounded final sample count. Never restart the file resampler for each microphone packet.
2. Fan original canonical PCM into advisory VAD and, when selected, enhancement. Use fixed internal 512-sample VAD steps, independently of capture packet boundaries, with the pinned model's 64-sample initial context. Pad/flush only the VAD's final inference frame, then translate and clamp annotations to real input coordinates. Synthetic VAD padding never reaches ASR.
3. Run DPDFNet through `OnlineSpeechDenoiser` in both application modes. Its tested 16 kHz exports use a 320-sample analysis window and 160-sample hop. Apply the benchmark-verified 640-sample output alignment, feed its required zero tail at Stop, flush, and require exactly one output sample for each canonical input sample. Empty intermediate output is normal. Do not use the offline-only attenuation-limit setting; online/offline startup padding already produced measured differences on short speech. [Official runtime contract](https://k2-fsa.github.io/sherpa/onnx/speech-enhancement/dpdfnet.html).
4. The existing worker exclusively owns the selected VAD/denoiser handles. These APIs combine weights and mutable state rather than exposing separately shareable streams. Reset all recurrent state, framing, offsets and pending results before each recording and after cancellation/failure; validate A/B/A isolation. Recreate on artifact/settings identity change and release on existing unload/shutdown. Only the selected denoiser remains resident.
5. Keep the existing bounded capture/inference queues. Drain VAD segment audio immediately after extracting intervals. Bound the native active-segment copy by flushing annotation bookkeeping every 320 VAD frames (10.24 seconds); mark these as bookkeeping splits, not ASR endpoints. Qualification must demonstrate continuous-speech coverage and bounded memory with this policy. Do not assume `max_speech_duration` forces a hard cut: the native implementation changes detection thresholds instead.
6. On queue saturation or a recoverable frontend failure, invalidate live partials, reset the session, and use retained original audio for the existing completed fallback. Do not splice raw PCM into a partially enhanced ASR session. Cancellation is checked between bounded native calls; a call already executing is not interruptible. Preflight validates artifacts, formats and finite samples, but the present native C API does not promise recovery from every internal exception.

True ASR partials remain available only for adapters that already implement live recognition, currently Nemotron. Cohere, Whisper and LFM can use the shared completed frontend without acquiring streaming decoder support. Processing audio during capture and producing incremental ASR text are different capabilities. The initial implementation does not add a second capture-processing worker for completed-only decoders; their selected frontend runs over the completed recording at Stop.

### Affected components and tradeoffs

Change `transcribe-audio` for the shared resampler; `transcribe-inference` for frontend state, worker sequencing and content-free stage evidence; `transcribe-core` and the existing model installer for opt-in settings and a frontend artifact role; and the desktop runtime/settings for configuration and recoverable fallback messages. Reuse the current installer, integrity checks, cancellation and settings controls. Add the exact tested artifacts to `models/artifacts.json` only behind their explicit experimental capability. Extend `transcribe-eval` to exercise the application path rather than importing the benchmark example into production. Record the accepted design in an ADR when approved.

The useful outcomes are consistent processing across modes, optional noise reduction, and measurements that can support future segmentation decisions. Advisory VAD does not itself save decoder work. Enhancement adds CPU work and buffering, and can remove speech information. Keeping original audio enables retry but retains the existing recording-memory cost. Resident frontend sessions trade memory for avoiding repeated loads. None of these changes establishes lower end-to-end latency, improved WER, or a state-of-the-art result before paired application measurements.

### Frozen artifacts and evidence classes

Use the hashes and download identities in the [frontend artifact manifest](../benchmarks/asr/audio-frontend-artifacts.json): `silero-v6.2`, `dpdfnet4`, and `dpdfnet8`. Silero's package revision is `7e30209a3e901f9842f81b225f3e93d8199902b1`, with weights from `bfdc0193023f121ea5b3cc7b176dbed570a68a59`. DPDFNet's tested author export revision is `3b0a4ee55df845f6afd6be7e3253cdf5dbcfe634`; do not substitute the newer March 20 exports. The existing native runtime is Sherpa 1.13.8, upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, integration build identity `67ce39e3334e098b428e0e94284c3b6a0846e014a5c08000c02ae28f393ebc7b`, patch hash `22d05a9249ede683930e3f47c9c69ebeed66d85aaa8925914dca4b310984bae3`. The patch includes the VAD endpoint range guard from ADR-0019; earlier benchmark build identities remain in their original reports.

| Evidence class | Basis for this proposal |
| --- | --- |
| Paper result | [DPDFNet](https://arxiv.org/abs/2512.16420v1) supports causal enhancement as an architecture; [Ochiai et al.](https://arxiv.org/abs/2404.14860v1) demonstrate that enhancement artifacts can hurt recognition. Their reported speed/quality figures do not transfer to this application. |
| Official capability | The pinned Sherpa VAD and online denoiser expose accept/run, flush and reset operations. The [VAD source](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/voice-activity-detector.cc) and [online STFT implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/online-speech-denoiser-stft-impl.h) define state and boundary behavior. |
| Local measurement | The [2,337-evaluation regression](validation/audio-regression-2026-09-16.md) found quiet-speech misses, denoiser word regressions, packet-dependent VAD endpoints and different offline/online enhancement startup. These failures remain regression cases. |
| Unverified hypothesis | An automatically selected enhancement profile improves varied real dictation within its latency budget. Local packet/tail/reset tests now support the shared implementation; that correctness result does not establish a general quality or latency improvement. |

This first scope is **frozen-weight application/runtime work** using current public APIs. Runtime-internal FFT/kernel changes, a new native scalar-VAD binding, decoder changes and new quantization formats are excluded. Training/checkpoint changes, including artifact-aware training, personal-voice learning or making Uhm causal, are also excluded; none is a setting for these downloaded weights.

### Validation and release gates

Before implementation benchmarks, freeze development/held-out manifests and configuration in the issue. Existing failed benchmark clips become regression controls; they cannot double as a newly untouched qualification set. Build first, then run measurements sequentially without competing builds or inference.

- **PCM contracts:** whole input, fixed packets and irregular packets must yield the same compensated resampled output, identical deterministic VAD annotations, and DPDFNet output within a predeclared floating-point tolerance. Test empty input, 1/159/160/161/511/512/513 samples, 44.1/48 kHz input, abrupt onset, speech at Stop, and impulses at both boundaries. Verify exact final length, finite samples and no synthetic tail delivered to ASR.
- **Lifecycle:** A/B/A runs, cancellation during input and finalization, unload/artifact change, malformed input, queue overflow and completed fallback. Run at least 100 mixed recordings and 30 minutes of paced replay. Require zero unexplained drops, duplicate samples, cross-recording output or growing pending buffers; report RSS over time rather than equating process peak with model size.
- **Quality:** paired original/advisory/enhanced runs through Cohere→writer and Whisper; exercise other executable adapters only in their qualified language/mode envelope. Include clean/quiet/whispered speech, short negations, names/numbers, fan/keyboard/TV/overlap, noise/music-only, singing and code-switching. Keep unsupported modes as explicit exclusions. Preserve failures and evaluate raw ASR and writing separately.
- **Performance:** record cold and resident loading, resampling/frontend/ASR/writer times, p50/p95/p99, frontend RTF, actual queue depth/drops, first partial, word emission, endpoint and stop-to-final. Distinguish microphone capture, paced replay and accelerated completed input. Record machine/backend/precision/threads, source and artifact hashes, power and thermal observations.
- **Proposed initial budgets on the reference M5 Pro CPU:** paced 20 ms frontend dispatch p95 below 10 ms and p99 below 20 ms, zero queue drops, frontend-only RTF below 0.5, and resident p95 first-partial/stop-to-final increases no greater than 100 ms against paired raw runs for live-capable ASR. Report completed-only frontend overhead separately; it processes the full recording at Stop. These are targets, not existing measurements or edge-device guarantees.
- **Promotion:** integration behind opt-in settings may ship only after correctness and isolation gates pass. Automatic/default enhancement additionally requires a held-out noisy-speech improvement with a paired 95% confidence interval excluding zero, a clean/quiet WER-difference upper bound no greater than +1 percentage point, no new annotated negation/number/entity loss, and no increase in observed noise false transcripts. An underpowered or failed test leaves the feature experimental. Any VAD rejection or filler-deletion policy requires a separate proposal and recall/meaning qualification.

## Current release eligibility

| Model | Actual release evidence | Status under the one-year requirement |
| --- | --- | --- |
| **Silero v6.2** | Weight-changing commit and release **6 November 2025**; 1,289,603-byte 16 kHz ONNX | Eligible; native tests use these weights pinned through v6.2.1. The February 2026 package release only changes dependencies. [Weight commit](https://github.com/snakers4/silero-vad/commit/bfdc0193023f121ea5b3cc7b176dbed570a68a59), [release](https://github.com/snakers4/silero-vad/releases/tag/v6.2). |
| **DPDFNet-4/8** | Checkpoint upload **18 December 2025**; current checkpoint hashes unchanged | Eligible current checkpoints. Tested native files exactly match the author's **10 March 2026** exports; newer **20 March** ONNX exports differ. Latest-export parity remains unverified. [Checkpoint upload](https://huggingface.co/Ceva-IP/DPDFNet/commit/49d9463ca1d44300a123f02decdec595ac7e54fe), [compatible export](https://huggingface.co/Ceva-IP/DPDFNet/commit/3b0a4ee55df845f6afd6be7e3253cdf5dbcfe634), [new export](https://huggingface.co/Ceva-IP/DPDFNet/commit/9adcf02b3c4ae0924a6dbb4cc2f9b582574be947). |
| **Uhm** | Current fine-tuned core ONNX artifacts present **1 July 2026** | Eligible recent fine-tune of an older DistilHuBERT backbone. August adds an optional type head; September card edits are not new core weights. [Release evidence](https://huggingface.co/desert-ant-labs/uhm/commit/b864ecf50e7ca27b813c2bdb67964a9ffe84c413). |
| **Hush** | ONNX bundle **19 March 2026**, PyTorch checkpoint **31 March** | Eligible recent fine-tune, **research only**; not locally qualified. [Official model](https://huggingface.co/weya-ai/hush/tree/a55d932cbf6344d284ac985f21e7f6e5bc4d38a5). |
| **Clear** | Repository created May 2026; fine-tuning stated in official notices; current snapshots September 2026 | Recent family, **research only**. Squashed history prevents an exact original weight-release date; v0.4.0's ONNX hashes equal earlier tags. [Fine-tune provenance](https://huggingface.co/desert-ant-labs/clear/blob/16be4d516113853a2100fbb295c32186b2a05606/THIRD_PARTY_NOTICES.md). |
| **Ear** | Deployment artifact released **25 August 2026**, based on retained/compressed Whisper-tiny weights | Explicitly requested and tested, but **not evidence of a newly trained checkpoint**. Keep this distinction visible under a strict recent-checkpoint shortlist. [Author description](https://huggingface.co/desert-ant-labs/ear). |

Silero v4, GTCRN, CED, DeepFilterNet3, original EfficientAT/PANNs, MarbleNet v2 (May 2025) and published TEN releases (July 2025) are older baselines. Recent exports or README edits do not reset checkpoint age. No well-supported, recent general event classifier emerged from this bounded search: a September 2026 193 KB room classifier has limited seven-class coverage and no music label; another recent EfficientAT derivative recognizes four unrelated events. Recency alone does not justify adding either. Exact evidence and exclusions are retained in the release audit.

## Different jobs need different evidence

| Component | Output and possible benefit | What it cannot establish |
| --- | --- | --- |
| Voice activity detection (VAD) | Speech activity and boundaries; may reduce non-speech decoding and improve segmentation | Which person is speaking, what they intend, or whether all quiet speech was retained |
| Speech enhancement / denoising | A modified waveform with reduced interference | Guaranteed transcription improvement or removal of another overlapping speaker |
| Audio event classification | Scores for events such as speech, music or laughter | Clean speech samples; one dominant class does not exclude other simultaneous sounds |
| Diarization | Anonymous speaker activity over time | Persistent user identity or source-separated audio |
| Filler detection | Candidate hesitation spans | Whether deleting a span preserves the intended meaning |
| Spoken language identification | Language probabilities for ASR routing | Speech presence, user intent, transcription, or code-switch timestamps |

For dictation, a small VAD and optional denoiser address the immediate problem more directly than a general audio model. A classifier should be added only if a measured residual failure category justifies it. Recognition of the owner's voice would require a separately evaluated enrollment/verification or target-extraction path; neither of the supplied examples provides that product behavior by itself.

## Historical baselines and exact runtime contracts

| Candidate | Size or complexity evidence | Contract and qualification boundary |
| --- | --- | --- |
| **Silero VAD v4, k2-fsa export** | Downloaded FP32 ONNX: **643,854 B**; INT8: **212,860 B** | Existing `sherpa-onnx = 1.13.8` Rust API. The export uses the 16 kHz branch, 512-sample input (32 ms), and separate recurrent `h`/`c` state. Compare both precisions; INT8 size does not establish accuracy or speed. |
| **GTCRN** | Downloaded streaming ONNX: **535,638 B**. Authors' updated implementation counts 48.2K parameters including fixed ERB mappings, with 33.0 MMAC/s. | Existing native online/offline denoiser APIs. Artifact metadata: 16 kHz, 512-sample square-root Hann window, 256-sample hop, explicit convolution/recurrent caches. Flush the tail and reset every recording. |
| **WebRTC VAD** | Embedded GMM tables; no neural model download | C implementation accepts mono PCM16 at 8/16/32/48 kHz in 10/20/30 ms frames. Useful minimum-cost baseline, but this project would need an additional reviewed binding/build path. Aggressiveness trades false detections for missed speech. |
| **RNNoise v0.1** | Original checkpoint: 87,503 signed 8-bit weights, about 85.45 KiB of weight payload | Native C; 48 kHz, 480-sample hop. A compact historical comparator, not a claim about current RNNoise. Resampling, sample scaling, DSP delay and tail handling add work. |

Silero artifact identity is important: these small files identify themselves as **v4 exported by k2-fsa with only the 16 kHz branch retained**. They are not the current upstream v6-era model. The pinned [export script](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/scripts/silero_vad/v4/export-onnx.py) and [native model handling](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/silero-vad-model.cc) define this candidate. Do not import the newer model's parameter count, language performance or state layout. Silero's upstream license is [MIT](https://github.com/snakers4/silero-vad/blob/41f03a954b841327835dea1ddb7bb28ae23ddc2c/LICENSE).

GTCRN's [ICASSP 2024 author poster](https://sigport.org/sites/default/files/docs/GTCRN_poster.pdf) reports 23.7K parameters and 39.6 MMAC/s for the paper architecture. Its [pinned official implementation](https://github.com/Xiaobin-Rong/gtcrn/tree/502ebfab64da7c4a9af78dcb9c6ceef1ebb01c73) explains the later count/MAC changes and removal of a feature shuffle that prevented streaming. Published enhancement metrics and an Intel CPU runtime observation do not establish WER or speed in this application. The implementation has an [MIT license](https://github.com/Xiaobin-Rong/gtcrn/blob/502ebfab64da7c4a9af78dcb9c6ceef1ebb01c73/LICENSE).

WebRTC framing and state come from its [pinned C API implementation](https://webrtc.googlesource.com/src/+/26b31d40fe8f13d76d70076223b11437ce0471a8/common_audio/vad/webrtc_vad.c) and [GMM/hangover code](https://webrtc.googlesource.com/src/+/26b31d40fe8f13d76d70076223b11437ce0471a8/common_audio/vad/vad_core.c). A 10 ms frame is not a 10 ms endpoint decision: model hangover and application confirmation add delay. RNNoise's [primary paper](https://arxiv.org/abs/1709.08243) and [v0.1 source](https://github.com/xiph/rnnoise/tree/cdf196b1e9de2f8ff1003328ebf9a4316477429d) describe the tiny historical model. [Current RNNoise](https://github.com/xiph/rnnoise/tree/70f1d256acd4b34a572f999a05c87bf00b67730d) uses a different architecture; its model/build archive is not the deployed weight payload. Neither comparator has been built or measured here.

### Supplied examples and their actual roles

**Nemotron-3-Diarization-preview (historical, separate artifact):** the public terms restrict it to internal evaluation on NVIDIA GPUs, prohibit production/redistribution, and restrict publication of evaluation results. Its listed checkpoint is 397,199,360 bytes, revision `9c9cb0cc3bee7c7839bb6e1cf4938e63fee539bc`. That preview is incompatible with this CPU/open-source deployment target. These restrictions must not be assigned to the [23 September general release](#nemotron-3-diarization-general-release--25-september-2026). Gated preview weights and detailed configuration were not accessed. Do not assign preview-specific parameter counts, latency or speaker limits from another Sortformer checkpoint. [Official preview terms and inventory](https://huggingface.co/nvidia/Nemotron-3-Diarization-preview/tree/9c9cb0cc3bee7c7839bb6e1cf4938e63fee539bc). The [Streaming Sortformer paper](https://arxiv.org/html/2507.18446v1) concerns speaker activity/cache tracking, not noise filtering or persistent owner recognition.

**Uhm:** this is a DistilHuBERT filler-span detector. The base is approximately 23.5M parameters, not an exact count of this fine-tune; the listed FP16 ONNX file is **47,047,128 bytes**. English training and unmeasured acoustic transfer do not qualify Hindi or code-switching. The [model card](https://huggingface.co/desert-ant-labs/uhm/blob/9b172658b2a20a21ad8ea44092f4d4b3ff2c69a4/README.md), [DistilHuBERT paper](https://arxiv.org/abs/2110.01900v4) and [inventory](https://huggingface.co/desert-ant-labs/uhm/tree/9b172658b2a20a21ad8ea44092f4d4b3ff2c69a4) provide different kinds of evidence; the base paper does not demonstrate Uhm's ASR benefit.

The [official SDK detector](https://github.com/Desert-Ant-Labs/desert-ant-core/blob/2aed9c6d6459fc1843ab59553965035954e40199/Sources/Uhm/Detector.swift) uses 30-second windows, 25-second hops and averaging over overlaps. **20 ms is output resolution, not causal streaming latency.** A concrete packaging mismatch was verified: [labels.json](https://huggingface.co/desert-ant-labs/uhm/blob/9b172658b2a20a21ad8ea44092f4d4b3ff2c69a4/labels.json) has five labels, while [compiled metadata](https://huggingface.co/desert-ant-labs/uhm/blob/9b172658b2a20a21ad8ea44092f4d4b3ff2c69a4/uhm.mlmodelc/metadata.json) has six, including `and`. Generic filler detection would still need meaningful-word/affirmation deletion tests. Treat it as optional post-recording annotation, not automatic waveform cutting.

Uhm has a source-available license, not its base model's Apache license. The [SDK license notice](https://github.com/Desert-Ant-Labs/desert-ant-core/blob/2aed9c6d6459fc1843ab59553965035954e40199/LICENSE.md) describes scale and competing-model-training restrictions; full authoritative terms were inaccessible during this research. It is therefore not a default choice for unrestricted open-source distribution or continuous learning.

**Other candidates:** TEN VAD has a small 129,534-byte INT8 release asset, but its [license](https://github.com/TEN-framework/ten-vad/blob/main/LICENSE) adds competition/deployment restrictions to Apache terms. A smaller file does not resolve that product fit. YAMNet's event tagging is broader, but its [official frontend](https://www.tensorflow.org/tutorials/audio/transfer_learning_audio) uses roughly 0.96-second feature patches; it should not hold the first speech packet while waiting for a classification. Speech plus music requires overlapping labels, not winner-takes-all rejection. DeepFilterNet's [native Rust implementation](https://github.com/Rikorose/DeepFilterNet/tree/d375b2d8309e0935d165700c91da9de862a99c31) remains a secondary enhancement comparator if tiny models fail quality gates, rather than an extra always-on model.

## Broader research inventory, including older baselines

Sizes below are decimal MB for selected deployed artifacts, excluding runtime binaries, activations and caches. A community conversion is not qualified by its source architecture's paper. The [inventory](validation/audio-frontend-candidates.json) records exact revisions, remote size/hash metadata, conversion authority, licenses and unexecuted status. Downloaded candidates have independent local hash verification in the [benchmark artifact manifest](../benchmarks/asr/audio-frontend-artifacts.json).

| Role / candidate | Selected payload | Engineering assessment |
| --- | ---: | --- |
| DPDFNet-4 / DPDFNet-8 | 11.71 / 14.62 MB native exports | Tested through existing sherpa APIs. Author HF exports have different bytes/metadata. The architecture adds dual-path modeling and enhancement training objectives; neither is an application switch for other weights. [Paper v1](https://arxiv.org/abs/2512.16420v1), [author release](https://huggingface.co/Ceva-IP/DPDFNet/tree/dd6818d00f50c836fed43a6243ebe49116de5964). |
| DeepFilterNet3 | 8.59 MB community ONNX graphs | Strong additional streaming comparator; official native Rust DSP/state pipeline must be reproduced. The three graphs alone are not a complete denoiser. [Paper](https://arxiv.org/abs/2305.08227v1), [official implementation](https://github.com/Rikorose/DeepFilterNet/tree/d375b2d8309e0935d165700c91da9de862a99c31). |
| MossFormerGAN / FRCRN | 38.85 / 161.05 MB | Author models fit; current ClearerVoice loading/preprocessing is a separate adapter, primarily completed-file comparison. MossFormerGAN's training discriminator is not needed for inference. [Official models](https://huggingface.co/alibabasglab/MossFormerGAN_SE_16K), [FRCRN paper](https://arxiv.org/abs/2206.07293). |
| SpeechBrain mimic-loss enhancer | 89.24 MB inference checkpoint | Training perceptual network would add 211 MB but need not ship. ASR-aware training results do not establish gains with frozen Cohere. [Official configuration](https://huggingface.co/speechbrain/mtl-mimic-voicebank/blob/5142933779578738d62d0d1f79290e824c8cd2fb/hyperparams.yaml), [paper](https://arxiv.org/abs/1803.09816). |
| Desert Ant Clear | 24.80 MB ONNX | A more directly relevant Desert Ant candidate for noise suppression than Ear/Uhm. Researched only; its own preprocessing, parity and source-available terms need evaluation. [Pinned model](https://huggingface.co/desert-ant-labs/clear/tree/16be4d516113853a2100fbb295c32186b2a05606). |
| CED tiny / mini / small / base INT8 | 6.15 / 10.47 / 22.64 / 87.48 MB with labels | Existing native CPU family; mini/base executed. Offline 527-event multi-label scores, not streaming event timestamps. Distillation is how these checkpoints were trained, not an enabled runtime optimization. [Paper](https://arxiv.org/abs/2308.11957v2), [official code](https://github.com/RicherMans/CED). |
| Zipformer-small / regular tagging INT8 | 27.05 / 68.40 MB with labels | Existing native event family, researched only. An encoder architecture name does not establish this classifier's WER or event latency. [Runtime documentation](https://k2-fsa.github.io/sherpa/onnx/audio-tagging/index.html), [encoder paper](https://arxiv.org/abs/2310.11230). |
| AST INT8 / PANNs Cnn6 / YAMNet | 90.62 / 23.70 / 7.55 MB selected exports | Additional event comparators with different window/frontend/runtime contracts. AST's roughly 10-second input or YAMNet's roughly 1-second initial patch must not block first ASR delivery. [AST](https://arxiv.org/abs/2104.01778v3), [PANNs](https://arxiv.org/abs/1912.10211v5), [YAMNet implementation](https://github.com/tensorflow/models/tree/master/research/audioset/yamnet). |
| pyannote segmentation 3.0 / Community-1 | 5.91 / 32.82 MB selected weights and PLDA | CPU/offline overlap/diarization comparators; gated access. Community-1's VBx/PLDA pipeline differs from sherpa's fast clustering, so published DER cannot be assigned to the native path. [Official Community-1](https://huggingface.co/pyannote/speaker-diarization-community-1/tree/3533c8cf8e369892e6b79ff1bf80f7b0286a54ee). |
| WeSpeaker ResNet34 / ECAPA / TitaNet | 26.53 / 83.32 / 101.62 MB | Speaker embedding candidates, not user-intent or denoising models. ECAPA's supplied complete loader selects 88.98 MB. Persistent owner verification needs enrollment, calibrated thresholds and overlap/replay evaluation. [WeSpeaker](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM/tree/f0c48c298fd835726c27956a5d617bad7115627e), [ECAPA paper](https://arxiv.org/abs/2005.07143), [TitaNet paper](https://arxiv.org/abs/2110.04410). |
| Brouhaha | 47.22 MB | Joint speech/SNR/reverberation-quality estimates could inform conditional processing. No enhanced waveform; bidirectional/window context and gated artifact correspondence remain unqualified. [Official model](https://huggingface.co/pyannote/brouhaha/tree/c93c9b537732dd50c28c0366c73f560c3a7aeb02). |
| MarbleNet multilingual VAD v2 | 0.50 MB | Small author comparator. Export expects features, so it cannot load as a generic Silero artifact; listed training languages do not include Hindi. Output resolution is not causal latency. [Official model/export](https://huggingface.co/nvidia/Frame_VAD_Multilingual_MarbleNet_v2.0/tree/9d25eb877d9edab5101ccc843edd04787a6c72c4). |

EfficientAT, DTLN, MetricGAN+, MP-SENet and Hush are also inventoried. NVIDIA RE-USE's payload fits but its official CUDA path and noncommercial terms do not fit the intended default CPU deployment. Official MossFormer2 (221.55 MB) and Dasheng (476.05 MB) exceed the selected payload budget; a community 4-bit MossFormer2 file is a separate, unqualified backend/artifact. These exclusions preserve the distinction between download size and executable compatibility.

### Ear and Uhm need different evaluation targets

**Ear** identifies 99 languages using a retained Whisper-tiny language subgraph; it does not transcribe or classify general noise. Its tested LiteRT bundle is **23,130,833 bytes**, revision `557fcde24e585cdaba69c6bcc24a6ad23f9064cd`. The supplied alternatives are compiled Core ML and LiteRT, with no ONNX artifact. The frozen-weight routing experiment uses an isolated LiteRT CPU interpreter, while any production runtime dependency remains a separate integration decision. [Model](https://huggingface.co/desert-ant-labs/ear/tree/557fcde24e585cdaba69c6bcc24a6ad23f9064cd), [Whisper backbone paper](https://arxiv.org/abs/2212.04356), [official LiteRT Python API](https://developers.google.com/edge/litert/migration).

The exact host frontend uses 16 kHz mono, padding/trimming to 480,000 samples **before** centered STFT, periodic Hann400, hop160, the shipped 80×201 mel table, and the first 3,000 frames. It computes power/log10 in float32, floors at `max_log − 8` decades, then `(value+4)/4`. The SDK's long-file selector chooses up to three speech-like windows and averages probabilities. The local probe validates its single-window frontend against the author's golden fixture; it does not claim to implement or qualify that long-file selector. [Pinned frontend and selector](https://github.com/Desert-Ant-Labs/desert-ant-core/blob/2aed9c6d6459fc1843ab59553965035954e40199/Sources/Ear/Frontend.swift).

Contrary to the card's description of a model-decided reliability flag, the inspected graph returns 99 logits. The SDK applies a host rule: top-two probability margin ≥0.25 and top language outside Norwegian/Swedish/Danish. This is not an unknown-language or speech-presence detector. Local tests include a confidently wrong Hindi→Romanian prediction and reliable flags on synthetic noise. Any routing experiment needs language-capability checks, speech evidence, uncertainty fallback and short/code-switch tests. No Ear-specific research paper was identified; its accuracy/compression claims remain publisher evidence. [Pinned SDK routing](https://github.com/Desert-Ant-Labs/desert-ant-core/blob/2aed9c6d6459fc1843ab59553965035954e40199/Sources/Ear/Ear.swift).

**Uhm** locally loads as input `[1,480000]` and output `[1,1499,6]` on ONNXRuntime CPU. Its six-class graph conflicts with the five-label sidecar and browser example. The official browser feeds raw PCM while the Swift detector normalizes each real window before zero-padding. The probe treats these as separate configurations, validates probabilities and crops padded frames; scalar filler probability uses `1 − p(not_filler)`. SDK span merging is reproduced, including its maximum constituent-run confidence. The experiment annotates spans and tests negative controls; it neither edits waveforms nor establishes natural filler precision/recall. Confidence is not evidence that removing “hmm,” “and,” or another span preserves meaning.

Both Desert Ant artifacts have source-available terms rather than unrestricted permissive model licenses. Internal testing does not resolve redistribution or future training rights. Their useful roles should be judged against measured routing/span errors and full process cost, not their shared publisher.

## Evidence: clearer sound can give worse words

Ochiai et al. investigate enhancement artifacts directly. On their real CHiME-3 test setup, Table VI reports **28.7% WER on noisy input versus 31.7% after SNR-trained enhancement**. Adding some original observation back improved that particular system. These are paper results for those models and recordings, not expected results for GTCRN, Cohere, LFM or Gemma. Their inference-time mixing experiment can be reproduced with frozen weights; their artifact-aware training loss needs a new checkpoint. [Primary paper, v1](https://arxiv.org/html/2404.14860v1).

Consequently, perceptual scores such as PESQ/STOI/DNSMOS are useful secondary diagnostics, but cannot replace ASR accuracy and meaning checks. VAD confidence is not a guarantee of speech absence. Both VAD and denoising can lose a quiet consonant, short negation, whispered number or word at Stop. A writing model cannot reliably reconstruct deleted acoustic evidence.

| Evidence class | Finding from this research |
| --- | --- |
| Paper result | Tiny enhancement is feasible; processing artifacts can harm ASR. Published metrics retain their original checkpoint/corpus/hardware limits. |
| Official model/runtime capability | Pinned native APIs expose stateful VAD and GTCRN enhancement. Export/sample/state contracts and licenses are inspectable. |
| Local observation | Pinned CPU screening and hard regressions now measure frontend execution, ASR errors, latency, process RAM and Uhm's metadata mismatch. The earlier source-only audit is superseded by the linked measurements. |
| Unverified hypothesis | A future conditional frontend policy improves dictation. The larger paired raw/DPDFNet comparison does not justify an always-on denoiser; no conditional policy has been qualified. |

## Pre-integration codebase audit

The audited application Git revision is recorded with individual source hashes because the workspace contains uncommitted changes. Existing behavior was inspected, not modified:

- [Capture](../crates/transcribe-audio/src/capture.rs) preserves original finite PCM, with no inference in the device callback. Its worker sends approximately 20 ms mono chunks at the device sample rate; queue saturation requests completed-audio fallback. Any frontend belongs outside the callback, and its delays must not block original audio retention.
- [PCM conversion](../crates/transcribe-audio/src/pcm.rs) mixes channels and resamples completed audio to 16 kHz, flushing the resampler tail. Repeatedly calling this completed-file resampler on individual live packets would reset its filter and is not a streaming frontend.
- [Completed-audio segmentation](../crates/transcribe-inference/src/segmented_audio.rs) chooses low-energy boundaries but retains quiet audio. Its exact-zero skip is not a VAD. [Desktop no-speech messaging](../apps/desktop/src-tauri/src/runtime.rs) uses amplitude only for diagnostic copy, not recognition gating.
- [Manifest validation](../crates/transcribe-eval/src/manifest.rs) now permits empty references only for `Scenario::Noise`. The benchmark reports false transcripts and inserted words/minute separately for raw, cleaned and personalized output; conventional WER is omitted on empty-reference cases. The legacy score helper's sentinel remains for compatibility, but is not used as noise WER. Regression tests cover this distinction.

### Native details worth profiling before adoption

The existing Rust dependency is `sherpa-onnx = 1.13.8`; its published upstream revision is `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`. The crate's VCS metadata says `dirty: true`, so the audit also records Cargo checksums and actual wrapper source hashes. Public API availability does not prove the installed native binary or selected artifact has passed this application's tests.

1. **Denoising includes more than the neural graph.** The pinned [online STFT implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/online-speech-denoiser-stft-impl.h) computes a direct DFT/inverse using nested loops and four double-precision trigonometric tables. At this artifact's FFT size, the tables alone imply 4,210,688 bytes (source-derived allocation, not measured RSS). Replacing it with an FFT is a potential native-runtime optimization, not an available application flag or a measured speedup.
2. **VAD segmentation copies can grow with speech duration.** The pinned [VAD implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/voice-activity-detector.cc) copies the current speech buffer during `AcceptWaveform`. It also changes silence/threshold settings after its configured duration threshold; that setting is not a simple guarantee of a hard segment cut. Profile sustained speech and queue draining. The current safe Rust wrapper exposes detected speech and segment queues, but not the native C++ scalar `Compute` method; do not promise zero-copy probability-only telemetry through that wrapper.
3. **Hop, output delay and endpoint hold are separate.** GTCRN's online implementation suppresses the first hop and flushes buffered output at the end. Native resampling adds its own state. A 16 ms enhancement hop or 32 ms VAD window is not the application's end-to-end latency. The safe online denoiser has `run`, `flush`, `reset`, sample-rate and frame-shift methods; all require qualification with the pinned binary/artifact.

## Paper-to-engineering classification

| Category | Work appropriate to this investigation |
| --- | --- |
| Frozen-weight application/runtime work supported by public APIs | Benchmark VAD decisions/segments and denoising; sequential chunk delivery; request reset and tail flush; original-audio bypass; padded boundaries; aligned original/enhanced mixing; stage/resource measurement. The shared application integration is accepted in ADR-0019. |
| Runtime-internal work | Efficient FFT replacement; removing duration-dependent VAD copies; exposing a scalar path if the required native binding is absent; platform-specific operators or new quantized exports. No such changes or speedups are implemented here. |
| Training/checkpoint work | Artifact-aware denoiser training, ASR/enhancer joint training, distillation, personal speaker extraction, or making Uhm causal. These are not switches for the downloaded models, and require separate data/validation/provenance. |

## Qualification protocol

The 25 September qualification reran the installed frontend contract with
Silero enabled and denoising off/DPDFNet4/DPDFNet8: all three configurations
pass 100 recording resets, cancellation/drop isolation, irregular packetization,
tail lengths and 44.1/48 kHz transitions. The
[current inference report](inference-performance.md) retains the named runtime,
resource observations and continuous-stream/completed-file soak separately.
These checks establish processing continuity; they do not establish a noisy-speech
WER improvement or clear the existing per-model default-enablement gates.

Use the [ASR qualification guide](../benchmarks/asr/README.md). Pin artifacts and frontend settings independently of ASR/writing artifacts. Compare **the same** held-out audio and languages on each actually supported adapter. Include the default Cohere → Qwen and supported LFM English path; Hindi, automatic-language and code-switching evidence must not be inferred from English. Keep real microphone tests, paced file replay and accelerated file throughput distinct.

The original research ablation included original audio, advisory VAD, padded VAD segmentation and the GTCRN baseline. The accepted integration qualifies original audio, advisory Silero v6.2, and Silero with DPDFNet4/8; segmentation/deletion and GTCRN are outside its scope. Compare FP32/INT8 VAD independently before preferring INT8. If full enhancement harms words, test a development-tuned mixture of original and enhanced samples only after verifying delay alignment. VAD-before-denoise versus VAD-after-denoise is an experimental choice, not an assumed correct order. Do not wait for an entire utterance merely to obtain its first live partial, and do not skip silence in a stateful ASR stream until its clock/state/endpoint contract supports that behavior.

Include speaker-disjoint natural speech, short commands such as “no,” numbers, names, quiet/whispered speech, code-switching, fan/keyboard/wind, far-field/reverberant audio, TV speech, overlapping people, digital silence, nonzero noise-only input, music-only input and intended singing. Synthetic mixtures at fixed development-selected SNRs help diagnosis but remain separate from natural recordings. FLEURS read-speech tests alone do not qualify this feature. No silent classification veto for singing or speech over music.

Record speech WER/CER and insertion/deletion/substitution counts; non-speech false-transcript rate and inserted words/minute; missed-speech duration, onset/tail clipping; negation/number/entity retention; and final writing meaning preservation. Retain failed requests in aggregates. Tune thresholds, pre-roll, hangover and mixing only on development data, then freeze before held-out tests. Report paired quality differences with uncertainty; an inconclusive small sample does not justify enabling a default.

For streaming and completed mode separately, log source/model/runtime hashes, hardware/OS/backend/precision/threads, corpus/language/license, input rate, chunk/window/hop, padding/endpoint settings, cold/resident state, frontend p50/p95/p99, first partial/word-emission/endpoint/stop-to-final and stage times, partial stability, queue growth/drops, failure/reset/cancellation isolation, original/processed length and timestamp alignment, RAM/VRAM, power and sustained thermal behavior. Small artifact size is only one acceptance dimension. Numeric product latency and quality budgets must be fixed in the experiment proposal before choosing a production configuration.

The concrete development-benchmark and conditional integration proposal, affected components, tradeoffs and approval status are recorded in [Issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42). ADR-0019 covers the explicitly approved shared frontend lifetime and optional enhancement. Speaker tracking, automatic filler deletion, training and Ear/Uhm runtime integration remain outside that scope.
