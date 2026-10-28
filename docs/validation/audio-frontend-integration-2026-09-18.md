# Shared audio frontend qualification — 18 September 2026

The integrated frontend preserves packet/tail/reset contracts, but the new quality evidence does **not** support automatic denoising or VAD rejection. Both settings remain off by default. DPDFNet8 improves the measured noisy-English Whisper subsets, while denoising sharply regresses quiet speech and produces three rejected incomplete decodes. This is CPU application-path evidence, not SOTA or edge-device qualification.

## Completed-audio quality

All 1,032 planned completed ASR+writer attempts are present: three ASRs × four profiles × 86 cases. There are three ASR completion-limit failures and 29 writing rejections (18 fidelity failures and 11 output-limit stops); rejected writing retains raw ASR. No frontend fallback, length mismatch, unfinished frontend state or logged native exception occurred in these ordinary-input runs. Failure-injection tests are separate.

Raw WER percentages on the 24 previously unlisted natural recordings per condition (lower is better):

| ASR           | Condition       | Original | VAD only | VAD + DPDF4 | VAD + DPDF8 |
| ------------- | --------------- | -------: | -------: | ----------: | ----------: |
| Cohere        | Clean           |     5.66 |     5.66 |        6.24 |        5.81 |
| Cohere        | 0 dB mixture    |     9.43 |     9.43 |       11.32 |        9.87 |
| Cohere        | ×0.03 amplitude |    66.76 |    66.76 |      67.52† |       66.33 |
| Whisper Small | Clean           |     7.84 |     7.84 |        7.84 |        7.55 |
| Whisper Small | 0 dB mixture    |    17.42 |    17.42 |       16.98 |       13.35 |
| Whisper Small | ×0.03 amplitude |    32.95 |    32.95 |       55.01 |      51.56† |
| Whisper Turbo | Clean           |     5.08 |     5.08 |        5.22 |        5.95 |
| Whisper Turbo | 0 dB mixture    |     9.43 |     9.43 |       10.16 |        7.40 |
| Whisper Turbo | ×0.03 amplitude |     6.53 |     6.53 |       53.27 |      18.80† |

† Scored-only WER on 23/24 clips; one additional clip failed completion. Cohere+DPDF4 hit `duration_token_limit`; Small+DPDF8 and Turbo+DPDF8 hit `model_token_limit`, all on attenuated input. Those clips are failures, not successful empty transcripts. Their reduced denominators and matched-pair exclusions remain explicit in JSON. Each complete 24-clip condition has 689 reference words.

VAD-only ASR, accepted writing output and error outcomes exactly match their original-input counterparts in all **258/258** completed pairs and **7/7** streaming pairs. VAD detects speech in 24/24 clean and 24/24 noisy held-out clips, but only 11/24 attenuated clips. This clip-presence observation is not frame-level recall; all input still reaches ASR, including the 13 misses. Attenuation is a signal transformation, not naturally whispered speech.

For the 0 dB subset, DPDF8 changes WER by −4.06 percentage points for Small (descriptive paired 95% interval −8.57…−0.17) and −2.03 for Turbo (−5.10…−0.14). These favorable results coexist with large quiet-speech regressions and do not satisfy the predeclared default-promotion gates. Cohere gains no noisy-speech improvement in this sample. All scored delivered-text WER values equal raw WER after accepted minimal writing or raw-text fallback; the writer does not repair recognition errors here.

False transcripts on eight noise controls:

| ASR           | Original | VAD only | VAD + DPDF4 | VAD + DPDF8 |
| ------------- | -------: | -------: | ----------: | ----------: |
| Cohere        |      0/8 |      0/8 |         0/8 |         0/8 |
| Whisper Small |      5/8 |      5/8 |         4/8 |         3/8 |
| Whisper Turbo |      7/8 |      7/8 |         7/8 |         7/8 |

This does not qualify Whisper for the application picker: its noise gate remains unmet. Advisory VAD cannot suppress those outputs because it intentionally does not remove audio.

Manual inspection of the four existing synthetic meaning controls found both Cohere denoisers introducing an action-verb substitution in the amount instruction that is absent from original/VAD input. Names, amounts, negations, dates and the explicit self-correction were retained in these controls; digit/punctuation normalization alone is not a meaning loss. All twelve spoken-instruction cases trigger writing fidelity rejection and retain dictated raw text. These are narrow task-authored controls, not natural semantic-fidelity estimates; synthetic WER/CER is excluded.

## Paced streaming and processing cost

All 28 short streaming attempts completed. The six resident rows per profile give the following descriptive timing observations; competing CPU activity makes them unsuitable for an uncontended speed comparison. These are application evaluator stages, not microphone-to-visible-text measurements.

| Profile     | First partial p50 / p95 (ms) | Stop-to-final p50 / p95 (ms) | Frontend dispatch p95 / p99 upper bucket bounds | Maximum dispatch (ms) |
| ----------- | ---------------------------: | ---------------------------: | ----------------------------------------------: | --------------------: |
| Original    |            1306.85 / 1561.53 |                65.54 / 72.38 |                                               — |                     — |
| VAD only    |            1307.98 / 1566.58 |                75.83 / 88.36 |                                      <1 / <1 ms |                 0.806 |
| VAD + DPDF4 |            1359.24 / 1360.57 |                80.60 / 84.85 |                                      <5 / <5 ms |                 5.287 |
| VAD + DPDF8 |            1353.61 / 1357.76 |                82.45 / 83.98 |                                      <6 / <6 ms |                 6.911 |

Dispatch observations include all seven rows. Total frontend native processing divided by audio duration is 0.0071 for VAD, 0.1689 for DPDF4+VAD and 0.2440 for DPDF8+VAD. Whole-process peak RSS is approximately 1.038 / 1.062 / 1.096 / 1.106 GB respectively; these are not frontend-only memory costs. Denoising changes recognition and the timing of the first nonempty result. The small first-partial p95 differences therefore do not demonstrate acceleration.

Completed clean-audio resident ASR-stage medians, including preprocessing and excluding writing, are 0.612 / 0.632 / 1.988 / 2.885 seconds for Cohere, 0.738 / 0.762 / 2.037 / 2.915 for Small, and 1.346 / 1.382 / 2.730 / 3.579 for Turbo (original / VAD / DPDF4 / DPDF8). Contention and changed decoder inputs preclude treating paired timing differences as isolated frontend overhead. The measured extra processing and mixed quality support keeping enhancement optional.

## Thirty-minute streaming soak

The separate VAD+DPDF4/Nemotron process completed warmup, 1,800 seconds of synthetic repeated speech and a fresh reset clip: three results, no missing rows, no runtime error and no native exception log. The long recording delivered exactly **28,800,000 input and output samples**, finalized successfully and retained 389 valid VAD intervals without omissions. The following recording's transcript exactly matches its pre-soak counterpart; all three sessions reuse frontend load ordinal 1.

The long row has 90,000 frontend dispatches, with p95 <5 ms, p99 <6 ms and maximum 51.679 ms. Seven dispatches fall in the ≥20 ms histogram bucket. Native frontend work totals 298.873 seconds (0.1660 × audio duration), first partial is 1,360.889 ms and Stop-to-final is 93.592 ms. These are contended observations, not a guarantee that every microphone deadline is met.

The evaluator retains **89,935 partial snapshots**, and peak whole-process RSS reaches 2.872 GB. Its reported streaming `asrMs` is 1,854,195.908 ms: the frozen timer includes post-finish partial-edit analysis and observer destruction. It therefore does **not** measure inference-only wall time or establish 54 seconds of feeder backlog. The original evidence is preserved; no estimated observer time is subtracted. First-partial and Stop-to-final are captured before that analysis. A bounded observer, an explicit finish timestamp and delivery-lateness measurement are needed before claiming sustained real-time performance or attributing this RSS growth to the frontend. This soak establishes long-input completion and reset correctness, not a clean latency/memory qualification pass.

## Scope and reproducibility

This is the accepted, opt-in integration in [ADR-0019](../adrs/0019-shared-audio-frontend.md), tracked in [issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42). Silero v6.2 supplies advisory intervals; DPDFNet4/8 optionally modify the waveform. Neither setting is enabled by default. Existing ASR architecture adapters share this stage. Ear and Uhm remain separate evaluation probes.

The content-free local debug report at `.cache/benchmarks/results/audio-frontend-integration-2026-09-18/debug-summary.json` preserves per-attempt counts, errors by stage, output hashes, exact artifact/runtime/source identities, process RSS, observed interference and descriptive paired intervals. Attributed public references and recipes are in the [frozen manifests](../../benchmarks/asr/manifests/frontend-integration-2026-09-18/quality.json). No private recordings were used; audio remains local. The preparation script rebuilds the published selection and checks its audio hashes instead of reselecting after seeing results.

The first reproduction attempt correctly rejected a file-hash mismatch: libsndfile rewrites creation time in FLOAT WAV's `PEAK` chunk even when decoded PCM is identical. The [container metadata sidecar](../../benchmarks/asr/manifests/frontend-integration-2026-09-18/wav-container-metadata.json) preserves the original timestamps for 73 generated files. Reproduction restores only those metadata bytes and still requires the full pinned file hashes. All 86 quality files, the streaming references and the 30-minute waveform then reproduced exactly. The initial failure log is retained; original corpus bytes, manifests and measured results were not modified.

- Twenty-four previously unlisted English FLEURS test recordings, four per sex × duration stratum: (2,8], (8,16], (16,25] seconds. Each appears clean, multiplied by 0.03 (−30.46 dB), and mixed with recorded environmental noise at nominal whole-clip 0 dB RMS SNR. Common headroom scaling prevents clipping; results do not isolate noise from gain changes. Noise sources: air conditioner, shutting door, typing and vacuum cleaner. Each natural condition contains 689 reference words.
- Four existing synthetic meaning controls, eight noise controls (four recorded, four synthetic), and two existing annotated AMI conversational windows bring the completed manifest to 86 cases. Existing controls are regression data, not fresh holdout. FLEURS does not expose speaker IDs here; neither speaker-disjointness nor exclusion from model training is established.
- Cohere, Whisper Small and Whisper Turbo run original / advisory VAD / VAD+DPDF4 / VAD+DPDF8, each followed by the configured Qwen writer. Order rotates by ASR. No threshold or checkpoint was changed after inspecting outcomes. English is explicit; this is not automatic-language, Hindi, French, code-switching, singing or naturally whispered-speech qualification.
- Nemotron Q8 runs four matched seven-row, 20 ms wall-clock-paced streaming manifests, including clean/quiet/noisy inputs and a reset repeat. A separate DPDF4+VAD lane runs warmup, 1,800 seconds of paced synthetic repetition and a fresh reset recording. Synthetic controls and soak are excluded from natural WER/CER; their failures and noise outputs remain counted.

Four initial streaming processes failed schema validation before inference because an unsupported split label was used. Their logs and process records remain preserved. Only the split metadata changed to `test`; waveform/reference/settings identities stayed fixed. Corrected manifest SHA-256 values: quality `b7845a9be3e2360011bc63277b2934971f815b4521c218670406542722da4ed2`, streaming `65b26feacc0d0052c9c6df78ade5654938de151a1e224c415fcf15214471f4d4`, soak `b5bb1457d8c505c62142ed107e4ea4cb4702d3fc1071000db21c7cf4eac2a7bd`.

## Measurement boundaries

Hardware is one Apple M5 Pro, 18 logical CPUs, 64 GiB, macOS 27.0 (26A428), CPU backend. Power source and `pmset` thermal observations are recorded per process. All completed quality/short-streaming jobs started on battery; the shared environment template retained a stale AC label, so the per-process observations are authoritative. `pmset` reported no recorded thermal/performance warning or CPU power status, which is not a temperature or throttling trace. Frontend graphs request one thread, Sherpa ASR eight, Nemotron four; BLAS/OpenMP caps and the disabled experimental prefix cache are recorded. INT8 Cohere/Whisper, Q8_0 Nemotron and the Q4_0 writer are separate precision envelopes from publisher results. When the optional stage is enabled, native-rate input is converted by the application’s Rubato resampler before ASR. The disabled live path may use the ASR runtime’s converter; 16 kHz VAD parity does not establish identical recognition between those converters. No GPU/NPU, physical microphone latency, other edge hardware, watts or temperature trace was measured.

The native frontend uses Sherpa 1.13.8 at `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, build `67ce39e3334e098b428e0e94284c3b6a0846e014a5c08000c02ae28f393ebc7b`, patch SHA-256 `22d05a9249ede683930e3f47c9c69ebeed66d85aaa8925914dca4b310984bae3`. The patch includes the narrow positive-endpoint VAD extraction guard. Exact ASR/writer/runtime bundle hashes are retained for every job. The benchmark executable and its 47-source-file inventory were frozen; a local matching source snapshot is retained. Subsequent narrow fallback/readiness/UI fixes are separately verified and inventoried, and are not mislabeled as that binary's source revision.

Completed ASR timing includes frontend work and ASR verification/load/decode when incurred, but excludes input-file decoding, capture/archive, UI delivery and later writing. The frozen streaming `asrMs` additionally includes post-finish partial-edit analysis and observer destruction; do not interpret its RTF as inference-only speed. Frontend verification/model-load fields describe one resident lifetime and must not be summed for every row. First-partial timing starts before stream setup and ends at the first nonempty provisional update; it is not per-word emission latency. Stop-to-final starts after all paced input has been delivered and includes queue draining, frontend tail and native ASR finalization. Replay submits each packet before waiting for its end deadline; packet availability therefore differs from a physical microphone callback by up to one 20 ms packet. No VAD-driven automatic endpoint is enabled or measured. Partial edit distance includes appended words as well as revisions, so it is not time-to-stable-word.

The streaming feeder uses bounded blocking delivery and cannot qualify physical microphone queue drops. Separate capture/queue unit tests exercise overflow fallback. Process RSS includes the loaded models, decoded original audio and the evaluator's retained partial snapshots; growing whole-process RSS alone cannot identify a frontend leak. A fixed 21-bin, 1 ms dispatch histogram bounds observer memory; its final bucket is ≥20 ms. Report percentile bucket bounds, not invented exact percentiles.

Observed competing CPU work is preserved in each process ledger. These measurements do not support an uncontended speedup, edge-device or SOTA latency claim. Bootstrap intervals use 5,000 paired recording-level resamples with a fixed seed; they are descriptive, unadjusted for multiple comparisons, and not speaker-level confidence guarantees. Scored-only WER is always accompanied by attempted/scored/failure counts; a decoder failure is never silently converted to a successful empty transcript.

## Qualification limits

The shared stage can feed existing architecture adapters, but this matrix only measures Cohere, Whisper Small/Turbo and native-streaming Nemotron. LFM and other supported ASR profiles need their own quality comparisons. Ear/Uhm integration, adaptive denoising, VAD-driven rejection or endpointing, physical microphone queue/latency measurements, natural whispers, broader languages and target edge devices remain outside this integration's evidence. No training or personal-voice learning is added. The default Cohere→writer pipeline and disabled audio-processing settings remain unchanged.

## Implementation verification

The measurement matrix is complete: **17 successful processes and 1,063 recorded attempts**, including the three ASR failures and 29 writing rejections above. No frontend length mismatch, unfinished state, invalid interval, dispatch-histogram mismatch, ordinary-input fallback or logged native exception occurred. Four preflight schema errors remain separately recorded.

Validation passed across the integration and final focused corrections:

- 139 focused Rust unit tests: audio 23, core 33, evaluator 17, inference 44 and model management 22. The final inference/model tests reran after the fallback/readiness changes. A desktop regression additionally passed for both explicit live failure and a dropped completion channel, including completed fallback failure and delayed partial rejection.
- Installed frontend packet/tail contracts: 45 lengths/profile combinations with two packet schedules, exact interval agreement and PCM tolerance ≤1e−6; 300 mixed reset/cancel/drop cycles and 24 bookkeeping-boundary pause cases. This suite ran on the integrated frontend before the narrow error/readiness/UI corrections.
- Three installed native live tests on the final source: cancellation/original-audio fallback/reset/unload; missing and corrupt frontend files without swallowing cancellation or ASR errors; and irregular 44.1/48 kHz delivery matching completed frontend processing for all three profiles.
- All 62 application JavaScript tests and two publication-boundary tests; strict Clippy for inference/models/desktop targets; workspace all-target checks; Rust formatting; Svelte with zero errors/warnings; Python Ruff; and byte-exact corpus reproduction.
- Browser preview checked saving/reloading the settings, keyboard selection/cancellation and the 390×844 layout. This is settings interaction coverage, not native microphone or acoustic quality coverage.

The JSON's `sourceHashes` identify the frozen measurement binary inputs. `postBenchmarkVerification` separately identifies the final corrected source and verification logs. The complete matching 47-file frozen source snapshot remains in the local benchmark cache. Native VAD guard validation passed separately. No files were staged or committed.
