# Conservative denoising and input-level qualification

This follow-up tests the required denoising feature using the existing frozen DPDFNet4/8 models and shared online frontend. Its predeclared protocol is recorded in [issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42#issuecomment-5735902954). No production default, adaptive routing, model training or native decoder changes are included.

## Completed main quality comparison

All 1,830 primary ASR/writer attempts are recorded. Fixed 50/50 mixing substantially reduces several full-suppression regressions, especially for Whisper Turbo, but it is **not qualified as an application default**. Quiet speech can remain worse than original audio; both Cohere mixtures introduce a noise-only false transcript. No production setting has changed.

Raw natural-speech WER percentages, lower is better. Each complete observed condition contains 24 recordings / 689 reference words. Parentheses show scored/attempted clips when ASR failed; failed clips are not successful empty transcripts.

| ASR | Observed condition | Raw | DPDF4 full | DPDF4 mix50 | DPDF8 full | DPDF8 mix50 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Cohere | Clean | 5.66 | 6.24 | 6.39 | 5.81 | 5.95 |
| Cohere | Noise mixture | 9.43 | 11.32 | 9.00 | 9.87 | 9.00 |
| Cohere | ×0.03 amplitude | 66.76 | 67.52 (23/24) | 66.91 | 66.33 | 67.63 |
| Whisper Small | Clean | 7.84 | 7.84 | 7.55 | 7.55 | 7.55 |
| Whisper Small | Noise mixture | 17.42 | 16.98 | 14.80 | 13.35 | 14.51 |
| Whisper Small | ×0.03 amplitude | 32.95 | 55.01 | 46.15 | 51.56 (23/24) | 33.68 (23/24) |
| Whisper Turbo | Clean | 5.08 | 5.22 | 5.22 | 5.95 | 5.37 |
| Whisper Turbo | Noise mixture | 9.43 | 10.16 | 6.68 | 7.40 | 6.97 |
| Whisper Turbo | ×0.03 amplitude | 6.53 | 53.27 | 11.32 | 18.80 (23/24) | 8.71 |

The fresh subset has 12 recordings / 322 reference words per complete condition and is reported separately:

| ASR | Fresh condition | Raw | DPDF4 full | DPDF4 mix50 | DPDF8 full | DPDF8 mix50 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Cohere | Clean | 4.66 | 4.97 | 4.66 | 4.35 | 4.66 |
| Cohere | Noise mixture | 8.39 | 11.49 | 7.76 | 9.01 | 8.70 |
| Cohere | ×0.03 amplitude | 78.57 | 80.75 | 79.81 | 84.78 | 80.12 |
| Whisper Small | Clean | 7.76 | 7.45 | 7.45 | 7.45 | 7.14 |
| Whisper Small | Noise mixture | 28.57 | 24.22 | 22.98 | 26.09 | 27.95 |
| Whisper Small | ×0.03 amplitude | 50.34 (11/12) | 72.67 | 57.76 | 63.27 (11/12) | 50.68 (11/12) |
| Whisper Turbo | Clean | 4.97 | 4.97 | 4.66 | 4.97 | 4.66 |
| Whisper Turbo | Noise mixture | 12.42 | 15.22 | 8.39 | 9.63 | 7.76 |
| Whisper Turbo | ×0.03 amplitude | 4.97 | 62.42 | 7.76 | 21.12 | 5.59 |

Turbo's DPDF8 mix changes fresh noisy WER by −4.66 percentage points, with a descriptive paired 95% interval reaching zero; its quiet WER increases by 0.62 points. The observed noisy change is −2.47 points. Small's DPDF8 full and mixed settings fail on different observed quiet recordings, so even their equal 23/24 scored counts do not establish identical comparison populations. Paired results retain these exclusions. Intervals use 5,000 fixed-seed recording-level resamples, are unadjusted for multiple comparisons and do not establish speaker-level generalization.

Noise controls show why favorable speech WER alone is insufficient. These are false transcripts / successfully scored noise clips; eight noise clips were attempted in every cell:

| ASR | Raw | DPDF4 full | DPDF4 mix50 | DPDF8 full | DPDF8 mix50 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Cohere | 0/8 | 0/8 | 1/8 | 0/8 | 1/8 |
| Whisper Small | 5/8 | 4/8 | 5/8 | 3/8 | 5/8 |
| Whisper Turbo | 7/8 | 7/8 | 6/7 + 1 ASR failure | 7/8 | 7/8 |

The failed Turbo noise clip cannot be counted as successful silence. Whisper's noise qualification remains unmet, and the new Cohere false transcripts block automatic adoption of either mixture.

All **774/774** reused raw/full-denoising cases match the earlier integration report's raw transcript hash, accepted writing hash and failure category exactly. This supports parity of the cached-waveform method for those cases, without establishing scheduling/timing parity. Manual review of the four synthetic meaning controls across all 15 primary lanes finds that both Cohere mixtures remove the action-verb substitution introduced by full denoising. Names, amounts, negations, dates and the self-correction remain intact in these controls. All 15 spoken-instruction cases reject writing and retain dictated raw text. This small synthetic screen is not general semantic qualification.

## Known-gain diagnostics

All 288 additional attempts on the 24 observed quiet recordings are recorded. Let `q = 0.03 x`, where `x` is the original recording. “Pre-gain, restore” is `0.03 D(q / 0.03)`; “post-gain” is `D(q) / 0.03`. Both gains were fixed before results. This reverses a known artificial attenuation; it is not an implemented AGC or a deployable streaming rule.

| ASR | Denoiser | Raw quiet WER | Direct quiet enhancement | Pre-gain, restore | Post-gain | Enhancement of original `x` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Cohere | DPDF4 | 66.76 | 67.52 (23/24) | 66.04 | 54.72 | 6.24 |
| Cohere | DPDF8 | 66.76 | 66.33 | 67.63 | 13.50 | 5.81 |
| Whisper Small | DPDF4 | 32.95 | 55.01 | 33.38 (23/24) | 54.28 | 7.84 |
| Whisper Small | DPDF8 | 32.95 | 51.56 (23/24) | 33.09 | 10.30 | 7.55 |
| Whisper Turbo | DPDF4 | 6.53 | 53.27 | 6.53 | 24.09 | 5.22 |
| Whisper Turbo | DPDF8 | 6.53 | 18.80 (23/24) | 6.68 | 8.71 | 5.95 |

The diagnostics establish a substantial low-level recognition problem and indicate level-dependent enhancement behavior. Cohere is already highly sensitive to the artificial attenuation without denoising: 5.66% clean WER becomes 66.76% quiet WER. Improving denoiser input level but returning its output to the quiet level leaves Cohere near 66–68%. Amplifying DPDF8's quiet output recovers much of its recognition quality, showing that useful speech information survives enhancement. For Turbo, improving DPDF4 input level and then restoring the quiet amplitude changes WER from 53.27% to 6.53%, with all 24 clips completing. The Small/DPDF4 pre-gain lane still has a completion failure; normalization is not universally qualified by these results.

The 24 diagnostic inputs have median RMS −83.32 dBFS. Direct DPDF4/8 outputs have medians −90.57/−83.72 dBFS; their median *paired* input-to-output changes are −5.17/−1.15 dB. Post-gain outputs remain at different levels (−60.11 versus −53.26 dBFS medians). Consequently, their residual WER gap does **not** isolate irreversible speech removal, specific artifacts or a runtime defect. Comparing gain-matched enhanced waveforms is the next isolating diagnostic. Any whole-recording level estimate must remain an offline diagnostic until a causal policy passes streaming, quiet-speech and noise-only controls.

All 36 original FLEURS files match the pinned public archive byte for byte. The very low levels are present in that archive and the declared ×0.03 transformation, rather than an extra decoder/extraction scale factor. These are extreme digitally attenuated recordings, not natural whispers or ordinary quiet-microphone qualification. The inspected export wrapper already applies spectral normalization; adding it a second time is not the proposed remedy.

Across the complete 2,118 completed-audio attempts, there are **nine ASR completion failures and 49 writing rejections**, with no missing rows or failed processes. Rejected writing retains raw ASR. Every natural-speech aggregate has identical raw and delivered WER here; the configured minimal writer does not repair the recognition errors in this experiment. Synthetic controls remain excluded from natural accuracy.

## Streaming and performance observations

All 35 Nemotron streaming attempts complete without errors. Each lane replays the same seven observed cases, totaling 54.72 seconds and 146 reference words **including a repeated reset recording**. This is a small regression fixture, not a fresh streaming accuracy benchmark. All five reset outputs exactly match the corresponding first recording. Enhancement worsens WER on this fixture:

| Profile | Fixture WER % | Stop-to-final p50 / p95 ms | Whole-process peak RSS GB |
| --- | ---: | ---: | ---: |
| Raw | 31.51 | 87.0 / 90.8 | 1.038 |
| DPDF4 full | 39.73 | 84.3 / 91.0 | 1.097 |
| DPDF4 mix50 | 37.67 | 85.7 / 88.8 | 1.097 |
| DPDF8 full | 37.67 | 86.4 / 92.0 | 1.108 |
| DPDF8 mix50 | 37.67 | 86.7 / 88.9 | 1.110 |

Stop-to-final starts at the nominal arrival time of the final real audio packet. It includes delayed delivery, the last packet's processing, frontend tail and ASR finalization. `feedFinishedToFinalMs` separately starts after the last enqueue. Across all lanes the maximum Stop-to-final is 94.0 ms and maximum observed packet-arrival lateness is 5.8 ms. These are contended replay observations, not physical microphone or production-placement guarantees.

Resident first-nonempty-partial medians are 1341.9 / 1378.9 / 1374.5 / 1372.9 / 1371.0 ms (raw / DPDF4 full / DPDF4 mix / DPDF8 full / DPDF8 mix), each on six recordings. First-recording values, including setup, are 1901.8 / 1710.7 / 1701.1 / 1712.5 / 1722.5 ms. Changed hypotheses and tiny sample counts prevent treating these differences as acceleration. The pinned streaming policy reports three right-context frames, 56 left-context frames and nominal 320 ms encoder advances; this does not equate that advance with user-visible latency.

For the four enhanced lanes, native frontend processing / audio duration is 0.1727 / 0.1613 / 0.2355 / 0.2358. All observed frontend dispatches stay below 20 ms. No input/output length mismatch, unfinished state, omitted VAD interval or invalid interval occurs. The original-audio FIFO peaks at 1,120 samples; one latest text snapshot is retained. These short recordings do not replace the separate long-soak qualification, and neither the replay audio nor ASR/transcript memory is bounded by that FIFO figure.

All **27 completed-audio and five streaming processes were contended**. Twelve completed lanes began on battery and 15 on AC; all five streaming lanes began on AC. Other workspace builds and system activity are recorded by process basename/CPU usage. A 2.07-second read-only public-archive audit also overlaps Turbo's DPDF4 mixture lane and is explicitly recorded. No benchmark compilation occurred during inference. Power-policy settings, watts and sustained temperature traces were not captured. The hardware was Apple M5 Pro, 18 logical CPUs, 64 GiB, macOS 27.0 (26A428), CPU backend; exact thread, precision, artifact and runtime identities are in the JSON.

The timings do not support an isolated speedup, an edge-device result or a SOTA claim. Mixing retains the full denoiser computation. Cached completed-audio ASR/writer timings exclude preprocessing, and the streaming probe's feeder placement differs from the application worker as described below.

## Verification and decision

The content-free local JSON at `.cache/benchmarks/results/denoising-followup-2026-09-19/debug-summary.json` contains **2,153 attempts and 34 process records**, with no missing jobs/rows, failed processes or logged native exceptions. Detailed generated evidence remains in the ignored cache under the repository's evidence-storage policy; this reviewed Markdown records the conclusions. The JSON preserves nine ASR failures (one duration-token limit, eight model-token limits) and 49 writer rejections (33 fidelity, 16 output-limit). Successful execution is not a quality qualification pass.

Verification passed:

- Two Rust mixing unit tests and strict Clippy for the development example; no production Rust behavior changed.
- Eighteen native fixed/irregular packet and tail comparisons, two dropped-session reset comparisons, and endpoint/impulse checks for both denoisers.
- Five publication tests covering failure denominators, synthetic exclusion, failed matched pairs, failed noise controls and private-path rejection.
- All 485 frozen source-file hashes and phase executable/input inventories; all 36 original public-archive files; byte-exact regeneration of the 122 selected inputs and frozen manifests.
- All 706 processed waveforms: pinned file hashes, finite 16 kHz mono PCM and exact lengths; 122 exact raw-PCM comparisons, 244 exact mixture/convex-bound comparisons and 48 exact post-gain comparisons. No original, enhanced, mixed or gain-diagnostic sample exceeded full scale. No clipping was applied to achieve this.
- Python formatting/lint and the five live reset repetitions. The complete native matrix ran sequentially from copied executables.

**Keep required denoising available as the existing explicit experimental option; do not promote a fixed mixture or automatic normalization from this matrix.** Conservative mixing helps several speech cases but introduces Cohere noise hallucinations and leaves quiet-speech regressions; it also regresses this Nemotron streaming fixture. Input/output level handling is the clearest next investigation, with a gain-matched diagnostic to separate residual attenuation from waveform changes. A causal candidate then needs fresh microphone-like quiet/noisy speech, noise-only, clipping/headroom, onset/tail, continuous-stream and target-device tests before production adoption. No adaptive routing, new denoiser, checkpoint training, decoder/kernel change or additional ASR adapter is implemented here.

## Fixed comparisons

The five primary profiles are original input, full DPDF4, 50% original + 50% aligned DPDF4, full DPDF8, and the same 50/50 mixture with DPDF8. The original weight is fixed before observing results. Mixing happens after the existing 640-sample alignment compensation; only real original samples enter the FIFO. Both input and output remain 16 kHz float PCM, with no clipping or peak/RMS normalization. Full and mixed waveforms reuse the same enhancement output for completed-audio comparisons.

The [frozen main manifest](../../benchmarks/asr/manifests/denoising-followup-2026-09-19/quality.json) contains the previous 86 observed regression cases and 36 variants of 12 fresh English FLEURS recordings. Fresh IDs are selected deterministically, two per sex × duration stratum, excluding previously recorded IDs. Clean, ×0.03 amplitude and nominal whole-clip 0 dB environmental-noise conditions use the earlier mixing recipe, including common headroom scaling. Fresh and observed results remain separate; file-ID novelty does not establish speaker/training-set disjointness. Synthetic meaning controls are excluded from natural WER/CER.

Five profiles × 122 clips × Cohere/Whisper Small/Whisper Turbo with the configured writer yield 1,830 main attempts. An additional 288 attempts isolate known-gain effects on 24 observed quiet cases. For quiet input `q = 0.03 x`, compare `D(q)`, `0.03 D(q / 0.03)`, and `D(q) / 0.03`, alongside existing original/clean/full profiles. The first comparison keeps nominal recognizer amplitude constant while changing enhancer input level; the last amplifies already enhanced output. Undoing a known artificial gain is a diagnostic, not a deployable AGC rule, and cannot restore information already removed by enhancement.

Five seven-case Nemotron CPU lanes measure paced streaming through the production frontend API plus the experimental mixing FIFO. Replay waits for each 20 ms packet to become available before submitting it. A bounded observer retains one latest partial; first-partial, feed duration, maximum delivery lateness, push wait, Stop-to-final and stream-finish timestamps are captured separately from scoring/serialization. These are replay measurements, not microphone or target-edge qualification.

The development probe enhances and mixes on its feeder thread, then sends processed PCM to the existing ASR worker. The shipped frontend runs inside that worker. Shared processing code and packet contracts do not establish identical scheduling or end-to-end latency between these placements; this probe qualifies the candidate waveform/streaming behavior, not production placement performance. It does not change inference ownership in the application. The first recording includes model setup; later recordings reuse that process's handles with fresh stream state. No automatic endpoint or queue-drop microphone simulation is enabled.

Completed-audio preparation queues the whole original clip when constructing the paired mixture; its memory observation is not a streaming FIFO bound. The live probe enqueues original audio one packet at a time and drains only sample-aligned valid enhancement. `mixUs` measures the acceptance/blending loop, excluding original-buffer insertion and unrelated pipeline work. Per-call integer-microsecond counters truncate fractional microseconds, so tiny summed mixing/push values are not precise estimates of total host overhead. Whole-process RSS also includes loaded models and the preloaded replay audio. Retaining one partial snapshot avoids an unbounded list of snapshots, but does not make the entire recording/ASR process constant-memory. Word-level emission/stabilization, microphone drops, automatic endpoint latency, watts and sustained temperature traces remain unmeasured.

## Research and runtime boundary

[Ochiai et al.](https://arxiv.org/html/2404.14860v1), Eq.17 and Proposition1, study observation adding. Their signal-to-artifact-ratio guarantee has a sufficient inner-product condition; it does not guarantee WER, intelligibility or meaning preservation. Their tuned weights are not an optimum for this application. The [DPDFNet paper](https://arxiv.org/html/2512.16420v1) describes gain augmentation and an over-attenuation loss; the latter requires training and is not a switch for downloaded weights.

The pinned Sherpa [online implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/online-speech-denoiser-dpdfnet-impl.h) does not apply its [offline attenuation limiter](https://k2-fsa.github.io/sherpa/onnx/speech-enhancement/dpdfnet.html#offline-attenuation-limit). Host mixing is an explicit experiment using its public processed-audio API. The inspected author [export wrapper](https://github.com/ceva-ip/DPDFNet/blob/99bbb521b0c87f63df1255b2dd05f6cdb5c50509/onnx_model/export_dpdfnet_to_onnx.py) already applies spectral input/output normalization; additional host normalization must not duplicate it. This inspected code revision is separate from the pinned artifact release in ADR-0019. Stateful statistics and numerical floors make amplitude sensitivity plausible, but only controlled local results can characterize this artifact/runtime combination. No PyTorch-versus-ONNX parity claim is made here.

Artifact/runtime pins remain those in [ADR-0019](../adrs/0019-shared-audio-frontend.md). The benchmark freezes its own application source and executable separately from the September 18 experiment and concurrent workspace edits. Preprocessing wall/native/mixing time is separate from ASR/writer time over cached processed WAVs; those stage observations are not automatically complete application latency. Process load, power source, precision, thread settings, failures and artifact identities are retained. No candidate is promoted from publisher claims or a scored-only aggregate that hides failures.

| Engineering category | Scope of this experiment |
| --- | --- |
| Frozen-weight application/runtime | Public processed-audio API, fresh frontend sessions, sample-aligned mixing, fixed known-gain transforms and measurement. Implemented in the development probe only. |
| Runtime-internal decoder/kernel | None. No online attenuation-limit implementation, ONNX graph, decoder, quantization or operator changes. |
| Training/checkpoint | None. AB-SDR and DPDFNet over-attenuation objectives motivate research; they are not enabled by a runtime setting. |

Paper findings and official implementation capabilities motivate the protocol. The result section reports local measurements separately; an adaptive gain/mixing policy remains an unverified hypothesis.

## Reproduction

Use the existing audio-eval NumPy/soundfile environment and public cached corpora. `scripts/prepare-denoising-followup.py` freezes input recipes/hashes and refuses to overwrite its output. `denoising_followup`, a development example in `transcribe-eval`, prepares matched processed corpora, validates mixing/packet/tail/reset contracts, and runs bounded-observer streaming. The standard application evaluator supplies completed ASR/writer results over those waveforms. Original audio and raw transcripts stay in the ignored benchmark cache; publication retains scores, output hashes and content-free diagnostics.

Before timing, build the example and evaluator with the configured native runtime, freeze the source inventory and copy both executables into the result root. Do not rebuild during native measurements. The sequential phase commands are:

```sh
python3 scripts/run-denoising-followup.py contracts
python3 scripts/run-denoising-followup.py prepare
python3 scripts/run-denoising-followup.py quality
python3 scripts/run-denoising-followup.py streaming
.cache/audio-eval-venv/bin/python scripts/verify-denoising-followup.py
.cache/audio-eval-venv/bin/python scripts/test_denoising_followup_report.py
.cache/audio-eval-venv/bin/python scripts/summarize-denoising-followup.py
```

Run waveform verification after native timing, then publish the summary with its verification results. The verifier reconstructs inputs in a disposable temporary directory and never replaces the measured corpus.

The runner refuses to overwrite unrecorded attempts. Quality and streaming freeze executable/input inventories before launch and verify them before/after each process. The initial contract/preparation processes used the preceding runner revision with post-execution binary hashes; that original harness and its fingerprint remain retained separately. Strengthening orchestration integrity did not change the native binary, selection, profiles or audio. Report hashes identify both harness revisions instead of attributing the first processes to the later checks.

The fixed experiment and verification are complete. Broader languages, natural whispers, a new long streaming soak, physical capture behavior and target edge hardware remain outside this evidence.

## Gain-matched diagnostic results

The earlier gains survive the stricter control. **720 main attempts plus 24 confirmation attempts** are recorded. At the same restored per-recording RMS, pre-gain DPDF4 reduces Cohere WER from 22.93% to 5.81%, Small from 12.34% to 7.84%, and Turbo from 6.24% to 4.93%. These are direct-enhancement versus pre-gain-enhancement comparisons on the same 24 artificially attenuated English recordings, not improvements over unenhanced input. The raw restored controls score 5.81%, 7.98% and 5.37% respectively.

WER percentages, lower is better. Each complete cell has 24 recordings / 689 reference words; a parenthetical denominator marks incomplete ASR scoring. “Pre-gain” uses the previously frozen pre-gain/restore waveform, then applies the common RMS match. Both denoiser arms therefore have the same final total RMS within each row.

| ASR | Matched output level | Raw | DPDF4 direct | DPDF4 pre-gain | DPDF8 direct | DPDF8 pre-gain |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Cohere | Quiet | 66.76 | 68.36 | 67.63 | 66.47 | 66.76 |
| Cohere | Restored | 5.81 | 22.93 | 5.81 | 12.34 | 6.24 |
| Whisper Small | Quiet | 32.95 | 55.30 | 33.24 | 52.69 | 33.09 |
| Whisper Small | Restored | 7.98 | 12.34 | 7.84 | 10.16 | 6.97 |
| Whisper Turbo | Quiet | 6.53 | 26.57 (21/24) | 6.24 | 14.38 (23/24) | 6.24 |
| Whisper Turbo | Restored | 5.37 | 6.24 | 4.93 | 7.69 | 5.81 |

Pre-gain DPDF4 versus direct DPDF4 at restored RMS changes WER by −17.13 / −4.50 / −1.31 points for Cohere / Small / Turbo. Descriptive paired 95% intervals are [−29.05, −7.01], [−11.83, −0.47] and [−2.63, −0.38]. They use 5,000 recording-level resamples and are unadjusted for multiple comparisons; these already observed clips are not a new holdout. All restored pre-gain comparisons against the corresponding **raw** control have intervals spanning zero. This supports recovering denoising regressions, not claiming a general accuracy advantage over raw speech.

**Interpretation:** recognizer input level and enhancer input level both matter. At quiet RMS, Cohere stays near 67% WER even with pre-gain enhancement. At restored RMS, boosting DPDF4's already-enhanced output brings Turbo to 6.24%, while Cohere remains at 22.93% until enhancer-input conditioning is changed. Equal global RMS therefore explains much of some regressions but not all of them. Remaining differences can involve envelopes, spectral balance or speech/noise proportions; this experiment does not prove a particular distortion or native-runtime defect. Restored raw is calculated from the stored float32 quiet recording and need not be byte-identical to the original clean recording.

All 72 unchanged quiet-raw controls reproduce the earlier raw/writer hashes and failure categories exactly. All 240 transformed waveforms pass exact scalar-PCM reconstruction, length, finite-value and hash checks; all 120 source waveforms and 485 frozen native source files verify. Maximum relative RMS error is 2.45e−9; no shared peak/gain reduction or clipping was needed. The largest applied gain was 1,511.06, which is a diagnostic observation, **not a safe automatic-gain recommendation**.

The main matrix retains four ASR model-token-limit failures (three direct DPDF4 quiet and one direct DPDF8 quiet, all Turbo), plus 12 writer rejections (six fidelity, six output limit). None of the restored-level ASR attempts fails. Delivered aggregate WER equals raw WER in every lane; rejected writing retains raw text. The confirmation adds three writer rejections with identical failure/output results; it is not added to the main quality denominator.

One of the 30 main processes tripped the shared-worktree source guard described below. Its 24 raw/writer hashes, scores and failure kinds match the separate frozen-harness confirmation exactly, with unchanged executable/manifest and clean confirmation integrity checks. The original event remains in the ledger and the strict `matrixComplete` flag remains false; all 720 planned main rows are present. There are no missing/duplicate rows, failed native processes or native exceptions. Six numerical/isolation and six reporting tests pass, including checks that a repeat cannot erase a guard failure or conceal changed outcomes.

All 31 processes started on AC; 29 main processes and the confirmation are contended. One uncontended lane does not establish a comparative speedup. The inherited M5 Pro/CPU, quantization, threads, model/runtime pins and native source revision remain authoritative. No streaming AGC, physical-microphone, noise-only, language-generalization, edge-device or latency qualification is implied.

**Decision:** pursue bounded causal level handling in the shared frontend; the promising accuracy recovery is real on this diagnostic. A production policy must still choose its gain without the original recording's known attenuation and pass fresh quiet/noisy speech, noise hallucination, onset/headroom and streaming reset/causality gates. Neither fixed mixing nor a normalization default is promoted. Detailed results, source inventories and generated manifests remain in the ignored `denoising-gain-matched-2026-09-19` cache.

## Gain-matched diagnostic protocol

The [subsequent fixed protocol](https://github.com/rahulsharmavishwakarma/transcribe/issues/42#issuecomment-5737484430) rechecks the promising pre-gain and post-gain outcomes while controlling one whole-recording energy difference. It reuses the 24 observed quiet clips and five cached waveforms: raw quiet, full DPDF4/8 quiet enhancement, and their pre-gain/restore outputs. Requested RMS targets are the raw quiet RMS and that RMS divided by 0.03. For each clip/level, one shared target is reduced if any arm would exceed peak 0.95 or gain 10,000. Every arm then receives a single positive scalar. No clipping, resampling or independent peak/gain limiting is used. Nonfinite, empty, mismatched or near-zero (RMS ≤ 1e-12) inputs are explicit unsuccessful preprocessing cases.

Five arms × two levels × 24 clips × three recognizers gives 720 planned ASR/writer attempts. `scripts/qualify-denoising-gain.py prepare` produces float WAVs, verifies serialized PCM and freezes source/binary/harness/input hashes in `.cache/benchmarks/results/denoising-gain-matched-2026-09-19/`. Its `run` phase reuses the sequential integrity/failure accounting in `run-denoising-followup.py` and the preceding frozen native evaluator. The original tested application revision remains authoritative; a newer Python driver does not make that binary a current-source build. Models and runtime are unchanged. Derived manifests and reports remain in the ignored cache; the existing tracked source selection is reused.

This is a completed-audio diagnostic using future audio and a known artificial gain. Equal total RMS does not equalize speech-active loudness, envelopes, spectral shape or SNR, and does not identify a runtime defect. [Le Roux et al.](https://arxiv.org/pdf/1811.02508), §2.3, show why a scalar-invariant waveform measure cannot expose gain errors; measured recognition remains necessary. This experiment does not reproduce Ochiai's reference-component decomposition or qualify a causal streaming controller. Noise-only, natural quiet speech and fresh-device gates remain outstanding regardless of its outcome.

The next causal design should reuse the shared frontend rather than introduce per-ASR gain adapters. Official [WebRTC AGC2](https://webrtc.googlesource.com/src/+/b32e4b35e6025f044e88f4cc3169523b3021e3a9/modules/audio_processing/agc2/adaptive_digital_gain_controller.cc) at revision `b32e4b35e6025f044e88f4cc3169523b3021e3a9` provides implementation references for framed gain updates, speech confidence, noise constraints and limiter feedback. Its [configuration](https://webrtc.googlesource.com/src/+/b32e4b35e6025f044e88f4cc3169523b3021e3a9/api/audio/audio_processing.h) places adaptive gain after noise suppression; these defaults do not qualify pre-enhancer conditioning or transfer directly to Silero. A candidate must preserve prefixes, packet-independent state, headroom and exact reset/tail behavior, and test sudden loud onsets, silence, continuous noise and first-word loss. Applying a time-varying inverse gain to a delayed, stateful enhancer cannot be treated as exact undoing.

Recent [SE-AGCNet research](https://arxiv.org/html/2606.25959v1) jointly trains enhancement and gain control with quiet-speech/noise objectives. Its bidirectional LSTM is a checkpoint/training approach, not a causal configuration switch for these DPDF weights. The paper's results are separate from this repository's measurements. No new model, dependency, WebRTC implementation or production gain policy is introduced by this diagnostic.

Current reproduction/check commands use the repository's locked Python project:

```sh
uv run --locked python scripts/qualify-denoising-gain.py prepare
uv run --locked python scripts/qualify-denoising-gain.py run
uv run --locked python scripts/qualify-denoising-gain.py verify
uv run --locked python scripts/test_denoising_gain.py
uv run --locked python scripts/test_denoising_gain_report.py
uv run --locked python scripts/summarize-denoising-gain.py
```

Preparation refuses an existing output directory. The launcher now enters an isolated execution directory backed by the frozen harness, binary and inputs. The measured run began with the preceding launcher revision: a concurrent commit hook temporarily restored the committed pre-refactor runner during one process, which tripped the source hash guard. That original attempt and guard record remain intact; remaining jobs resumed using the original frozen harness, and a separate confirmation repeats the affected comparison. The report preserves this event rather than claiming an uninterrupted integrity pass. Both orchestration revisions and the measured Python/NumPy/soundfile versions are retained locally; updating the launcher does not relabel the measured native source revision.
