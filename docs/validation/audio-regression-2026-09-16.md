# Hard audio regression — 16 September 2026

The larger paired ASR test **does not justify an always-on denoiser**. Preserve original audio. Silero and Ear have useful decision signals, but neither supplies a reliable veto for discarding audio or choosing a language without fallback. Uhm needs stronger false-positive and latency qualification before automatic removal. The largest measured short-request cost is currently artifact verification, followed by recognizer loading—not the decoder.

This extends the [initial screen](audio-frontends-2026-09-16.md), under [issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42). **All 39 main runs completed: 2,337 model/clip evaluations, no missing rows and no runtime errors.** Quality and cross-API contract regressions below still prevent promotion. The [frozen protocol](../../benchmarks/asr/audio-regression.md) specifies selection, corruptions, denominators, source licenses and reproduction. The [machine-readable results](audio-regression-2026-09-16.json) retain individual failures, paired comparisons, hashes and stage measurements without transcripts. Original recordings and raw reports remain in the ignored benchmark cache. Production defaults, filtering and inference lifetimes are unchanged; nothing was staged or committed.

## Scope and evidence limits

The corpus adds **24 previously unused English and 12 previously unused Hindi FLEURS test recordings**, balanced across published sex metadata and three duration bands. Ten signal versions of those recordings and four existing synthetic controls, plus eight noise negatives and a reset repeat, give **409 frontend entries**. These are variants, not 409 independent speakers. Two English recordings share a sentence, so paired bootstrap intervals use 23 reference-sentence blocks. No speaker-disjoint or model-training-disjoint claim is made.

The ASR matrix contains **363 Cohere→Qwen attempts**: raw, DPDFNet4 and DPDFNet8 on English original/attenuated/+10 dB/0 dB inputs, synthetic meaning controls, noise negatives and resets. Hindi is evaluated by the frontends and Ear; the current Cohere adapter does not qualify Hindi. Twenty-four AMI conversational windows add natural filler/backchannel evidence. AMI is one meeting/four speakers, a local validation selection rather than the official test split. Its approximate annotations do not establish precise event-boundary F1.

Recordings contain real source speech, but noise addition, attenuation, clipping, packet loss and reverberation are artificial corruptions. For three English sources and all four synthetic controls, noise mixing also reduces gain to prevent clipping. No matched-gain clean ASR baseline was run, so clean-to-noisy differences do not isolate noise alone. **Within-condition raw-versus-enhanced comparisons use identical PCM.**

All inference jobs ran sequentially on **Apple M5 Pro, 18 logical CPUs, 64 GiB RAM, macOS 26.6.2 (25G83), AC power, low-power mode off**. Frontends request one CPU thread; Cohere/Qwen request up to eight. There were no competing model runs or compilation during model timings; light inspection/analysis and unrelated system load were not controlled. This does not qualify mobile, x86, GPU, NPU, Core ML, power consumption or thermal behavior.

Application HEAD is `592e0084fe5cf486b4d56c575423e9a4f219bfb8` plus unstaged source. Each report pins its source/executable/manifest. Native runtime is sherpa-onnx 1.13.8, upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, with local build/patch identities in the JSON. Cohere INT8 export is pinned at `156a470cf08eefe706a0004f3c52d9ee567ca7a0`; its four artifacts total 2,888,052,036 bytes and are verified before every ASR job. Qwen3.5-0.8B Q4_0 and the exact llama.cpp/runtime pins remain in the same model-version catalog. Python probes use ONNXRuntime 1.30.0 and LiteRT 2.2.0. No instruction-level operator placement profile was collected.

Eligible recent checkpoints remain Silero v6.2 (November 2025), DPDFNet4/8 (December 2025) and Uhm (July 2026). Ear is an explicitly requested August 2026 deployment of older Whisper weights. DPDFNet uses the current checkpoint lineage through the author's compatible March 10 exports, not the different March 20 re-exports. Exact payloads, hashes and recency evidence are in the [artifact manifest](../../benchmarks/asr/audio-frontend-artifacts.json) and [release audit](audio-frontend-recency.json).

## ASR quality and meaning

Natural English WER is total word edits divided by **610 reference words across 24 recordings per condition**. Lower is better; this is the same checkpoint/backend/precision/corpus for each paired comparison.

| Input condition | Raw audio | DPDFNet4 | DPDFNet8 |
| --- | ---: | ---: | ---: |
| Original clean | 9.67% (59 edits) | 9.02% (55) | 9.51% (58) |
| +10 dB environmental mixture | 10.49% (64) | 10.49% (64) | 10.82% (66) |
| 0 dB environmental mixture | **14.59% (89)** | 14.92% (91) | 15.90% (97) |
| Extreme attenuation, ×0.03 | **69.84% (426)** | 76.56% (467) | 77.70% (474) |

Small clean gains are inconclusive. DPDFNet4's clean delta is −0.66 percentage points, with an unadjusted paired 95% bootstrap interval of −1.98 to +0.51 points. At 0 dB, its delta is +0.33 points (−4.76 to +4.66); DPDFNet8's is +1.31 (−3.76 to +5.53). At 0 dB, DPDFNet4 improves five recordings and worsens ten; DPDFNet8 improves three and worsens ten. These descriptive intervals do not establish universal superiority or inferiority. They do show that the earlier two-clip enhancement benefit did not persist on this larger selection.

The four synthetic controls are separate from natural accuracy. Original input has 8/52 word edits on clean controls; both denoisers have 9/52. Some counted edits are equivalent number formatting, but the specific verb regression is real: “Keep the total” becomes “Geek the total” with DPDFNet4 and “Eat the total” with DPDFNet8. Both survive the writer stage. This is why better-sounding audio cannot substitute for word/meaning checks.

Raw audio and DPDFNet4 produce no text on all eight noise negatives. DPDFNet8 produces “What?” on a vacuum-cleaner control: **one false transcript in eight**. Noise-only cases are reported as false transcripts and inserted words, not conventional WER with an empty denominator.

All expected ASR rows are present with no raw runtime errors. Empty ASR outputs count as full deletions. **Seventeen writer-fidelity rejections** remain explicit, and the retained-result metric uses accepted cleaned output or otherwise raw output. The accepted-writer-only metric has a smaller denominator and must not be used to hide empty recognition or fidelity rejection. Retained-result natural-English WER equals raw WER in this matrix.

Signal improvement did not predict better words: at 0 dB, median SI-SDR gains were **11.88 dB for DPDFNet4 and 12.08 dB for DPDFNet8**, with all 24 English waveforms improving on that metric. Nevertheless, their word-error totals increased. These are zero-mean SI-SDR comparisons against matched clean signals with no time realignment, not a perceptual or meaning score.

## Extreme low-level diagnostic

English original RMS spans −69.73 to −21.80 dBFS, median −50.65. After ×0.03, the range is **−100.19 to −52.26 dBFS**, median **−81.10**. This is extreme amplitude stress, not ordinary quiet or whispered dictation. Raw Cohere emits empty text for nine of the 24 attenuated recordings.

A separate five-case diagnostic, selected after observing failure and excluded from qualification accuracy, tests original/attenuated/restored/attenuated-repeat/original-repeat. The selected clip is −64.71 dBFS originally and −95.16 dBFS attenuated. Original/restored/repeated-original have zero word errors; both attenuated runs produce empty **EOS** outputs, without input truncation. This excludes the native silence short-circuit for those two calls. Restoring volume restores the words; it does not identify the responsible operation or validate automatic gain control.

The pinned filterbank clamps mel power at float epsilon before taking its logarithm. Fixed floors can destroy relative information at very low levels; the official Transformers extractor also differs in its log guard and several defaults. This is a **source-grounded hypothesis**, not a proven cause. Exact gated checkpoint-processor parity and encoder/export/INT8 isolation remain untested. [Pinned filterbank](https://github.com/csukuangfj/kaldi-native-fbank/blob/b09e686fe2084732ddd30d1ef80acfc0f13eaf01/kaldi-native-fbank/csrc/feature-fbank.cc), [official extractor source](https://github.com/huggingface/transformers/blob/276f1402020831d949c2e1a80574a5603995de23/src/transformers/models/cohere_asr/feature_extraction_cohere_asr.py).

## VAD and language decisions

Silero v6.2 detects speech in all 36 natural clean recordings and rejects all eight noise controls. On ×0.03 attenuation it misses 15/24 English recordings, while detecting all 12 Hindi recordings; differing input levels prevent a language-causal interpretation. At −5 dB it misses 2/24 English, 1/12 Hindi and 1/4 synthetic clips. These are whole-clip speech-presence checks, not annotated word-boundary recall.

The paired VAD counterfactual would suppress six attenuated clips that currently yield nonempty Cohere text. Total WER happens to remain unchanged on this set because those outputs are already poor; it provides no quality gain here and does not establish a safe absence-of-speech guarantee. The experiment never trims or discards the waveform.

Ear correctly identifies **24/24 clean English and 12/12 clean Hindi** recordings. At 0 dB it remains correct on 24/24 English but falls to **7/12 Hindi**, including one wrong prediction marked reliable. Three of eight non-speech signals also pass the SDK reliability heuristic. This supports further evaluation as a language hint with fallback, not unconditional routing. Ear's golden frontend parity check remains required; its multi-window long-file selector and code-switching behavior are untested.

## Natural filler and meaning checks

AMI annotations identify 12 uh/um anchors and 13 filler events in their windows. Counts below require actual temporal overlap with the annotation. The six backchannel windows and three laughter windows contain overlapping speakers; the 12 positive windows and three ordinary-speech controls do not. Reference times are approximate and all windows come from one meeting.

| Uhm input policy | Uh anchors hit | Um anchors hit | All filler events overlapped | Backchannel anchors marked | Ordinary-speech windows with a span |
| --- | ---: | ---: | ---: | ---: | ---: |
| Raw PCM | 4/6 | 6/6 | 10/13 | 0/6 | **2/3** |
| SDK normalization | 4/6 | 3/6 | 7/13 | 0/6 | **1/3** |

Normalization reduces some false alarms but also reduces positive detection on these cases. Neither setting marks a laughter anchor. Some predicted spans occur elsewhere in backchannel windows; these are reported separately from anchor overlap. The ordinary-speech alarms are relative to AMI's annotations, not a new exhaustive human relabeling.

Both configurations detect the synthetic um/uh controls and also mark the meaningful synthetic “hmm” affirmation. Neither marks the synthetic “and” control. On the five-second hum-only signal, raw/normalized input produces 5.00/4.98 seconds of filler spans. Natural backchannel success does not cancel these counterexamples. **No automatic filler deletion is justified.** Uhm's actual six-class output and five-label sidecar discrepancy also remains unresolved; the probe uses the author-style scalar `1 − p(not_filler)` and never invents class mappings.

## CPU latency and memory

RTF is frontend computation divided by input duration, excluding loading, file I/O, pacing sleeps and ASR/writing. Per-call p95 is not utterance latency. RAM is **peak whole-process RSS**, including the benchmark and retained result data; it is not incremental model memory. Pacing changes CPU scheduling/wake-up behavior, so its measurements are kept separate from accelerated throughput.

| Frontend | Payload | Completed-file sweep | Peak process RSS in that sweep |
| --- | ---: | ---: | ---: |
| Silero v6.2 | 1.29 MB | RTF 0.00215 | 480.2 MB |
| DPDFNet4 | 11.71 MB | RTF 0.10399 | 132.5 MB |
| DPDFNet8 | 14.62 MB | RTF 0.17379 | 138.2 MB |
| Ear | 23.13 MB bundle | Median inference 74.02 ms + preprocessing 2.48 ms | 271.6 MB |
| Uhm, raw | 47.05 MB FP16 artifact | Median inference 1,843.95 ms | 1,470.4 MB |
| Uhm, normalized | Same artifact | Median inference 1,843.81 ms | 1,482.9 MB |

Uhm/Ear pad short recordings into a fixed 30-second model window; these call times are not per-second streaming costs. FP16 artifact storage does not prove native FP16 arithmetic. Silero's long-sweep RSS differs substantially from its three-recording paced process below; no model-only allocation or leak diagnosis follows from these process maxima.

| Paced frontend, 20 ms delivery | Call p50 / p95 / maximum | Compute RTF | Maximum delivery lateness | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| Silero v6.2 | 0.189 / 0.611 / 5.094 ms | 0.01155 | 12.49 ms | 117.3 MB |
| DPDFNet8 | 6.921 / 8.633 / 12.314 ms | 0.33426 | 10.02 ms | 157.2 MB |

Each paced run includes a 180-second stitched clean/noisy/attenuated stream between two identical 4.62-second recordings: 189.24 seconds and 9,462 packet calls. Accelerated chunk replay is faster—DPDFNet4/8 call p95 is 2.224/3.799 ms—but must not replace paced measurements. This is synchronous frontend replay. First ASR partial, word emission, app endpoint/stop-to-final, real microphone queue drops, cancellation during native calls and sustained thermals remain unmeasured.

## Boundary and reset contracts

All **153 boundary calls** return finite output of the expected length. All **14 within-process, same-schedule reset comparisons** match. Another 240 repeated denoiser input/output comparisons match between standalone and ASR runs. However, equal sample counts do not establish full API equivalence:

- **VAD packetization:** fixed 320-sample delivery emits one segment ending at sample 69,152; the irregular schedule emits the same start but ends at 69,664, a **512-sample/32 ms difference**. A separate three-call fresh/repeat diagnostic reproduces the variable result exactly, ruling out reset leakage for these calls. The nine external boundary-reference comparisons include this cross-schedule difference; they are not all same-process reset tests. Pinned native code ORs per-window speech decisions within an incoming call and updates segmentation once using that call's buffer tail. The 1,600-sample packet spans several VAD frames. A future adapter needs deterministic internal delivery rather than assuming arbitrary callback batches are equivalent. [Exact native implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/voice-activity-detector.cc).
- **Denoiser startup:** fixed and variable online packetization produce identical PCM, but completed-file versus online processing differs on abrupt, short inputs. Maximum relative L2 differences are **0.551 for DPDFNet4 and 0.370 for DPDFNet8**; at the roughly one-second hard cut they are 0.0576/0.0153. These are not mere rounding differences. On the full 4.62-second reference clip they fall to about 2.2×10⁻⁷. The offline path honors centered/reflected STFT metadata; the online path starts with zero analysis history. In a separate diagnostic, prepending the same 320 zeros to both inputs and discarding the corresponding output prefix reduces the one-second discrepancy to **4.12×10⁻⁷ / 9.21×10⁻⁷**. This supports startup conditioning as the explanation for that case; it is not a general API-parity or ASR-quality qualification. [Verified native sources and actual artifact metadata](audio-regression-boundary-sources.json).

The original ASR matrix and model settings were not changed after these findings. Three VAD, eight denoiser-startup and five Cohere diagnostic calls are excluded from the main 2,337 evaluations and qualification denominators.

## The concrete inference optimization opportunity

**17 September follow-up:** the [paired application qualification](application-sha-2026-09-17.md) implements and tests Apple ARM hardware SHA while retaining all integrity checks. Its battery-powered results are a separate envelope; the AC-powered measurements below remain historical evidence. Cohere residency was subsequently approved and [qualified separately](cohere-residency-2026-09-17.md).

The separate five-case Cohere probe preserves current per-job recognizer ownership. Medians are **4.843 seconds for full artifact verification**, **1.641 seconds to create the recognizer**, and **0.294 seconds for decode**, with decode ranging 0.210–0.309 seconds on this 4.62-second clip. Waveform acceptance is about 1.2 ms. The larger raw-clean matrix has ASR-stage p50 7.164 seconds; that stage includes verification/loading/segmentation and is not a warm decoder number. Writer p50 is 0.365 seconds, with a first-load maximum of 7.881 seconds. Full ASR/writer process RSS reaches 6.10 GB.

The actual Cargo feature graph selects `sha2 0.10.9` default/std. On this ARM target that uses software SHA; the crate's supported `asm` feature enables CPU-detected hardware SHA. An **isolated nested Cargo workspace**, without changing the app build, tests both implementations against all four Cohere artifacts over three order-rotated passes. [Official dispatch source](https://docs.rs/sha2/0.10.9/src/sha2/sha256.rs.html).

| Full-file read + SHA-256, 2.89 GB | Three passes | Median |
| --- | --- | ---: |
| Current software backend | 4.887 / 4.739 / 4.743 s | **4.743 s** |
| Supported hardware backend | 0.988 / 0.961 / 0.964 s | **0.964 s** |

This is a **4.92× improvement in that verification microbenchmark**, with matching pinned hashes on all 24 file checks. All 32 padding/block/reader-boundary and mutation/truncation/oversize checks pass. Every file is still fully read and verified; no cached success or skipped integrity check is introduced. CPU SHA support is detected locally. Both builds use sha2 0.10.9; the isolated lock pins sha2-asm 0.6.4. Per-file timings exclude process startup/file open, the OS cache is not flushed, and three repeats do not establish a deployment-wide confidence interval.

**First follow-up:** qualify the supported SHA backend in the real application build and repeat end-to-end latency/integrity tests. That is a smaller change than redesigning model ownership. **Second:** propose resident Cohere weights with fresh recording/segment state and explicit unload/configuration behavior. The probe quantifies today's loading cost; it does not measure a resident implementation. Attention/KV/FFT optimization should follow operator profiling after these larger measured costs are addressed. No 4.92× ASR/application speedup is claimed.

## Validation and remaining work

Passed **24 Rust tests**, evaluation-target Clippy with warnings denied, isolated hash-target Clippy, formatting checks, **13 Python regression tests**, Ruff, official Ear frontend parity (maximum error 1.75×10⁻⁵ against tolerance 0.002), paired-input hashes, finite/sample-count checks and all same-schedule resets. Unequal-length/nonfinite comparison failures and rejected/empty transcript denominators have explicit regression tests. Main reports have no missing cases or runtime errors; 17 writer rejections and the quality/contract counterexamples remain recorded.

This completes the requested hard benchmark and conclusion, not production qualification. Deferred work includes additional speakers/sessions and naturally noisy or whispered speech, multilingual/code-switched/sung input, exact annotated VAD boundaries, short-command hard starts, checkpoint/frontend parity, actual live-ASR queues/cancellation, other CPU/edge hardware and power/thermal testing. No thresholds were tuned or models promoted using this validation set.

## Research interpretation

[Ochiai et al.](https://arxiv.org/abs/2404.14860v1) motivate measuring enhancement distortion against ASR accuracy. [Liu and Peng](https://www.isca-archive.org/interspeech_2020/liu20c_interspeech.html) motivate accounting for correlated ASR errors in bootstrap comparisons. [Le Roux et al.](https://arxiv.org/abs/1811.02508v1) motivate the secondary SI-SDR diagnostic. These paper results are not performance claims for our models. The local measurements above, official runtime capabilities and unverified explanations remain separate.

Frozen-weight/public-API work includes the benchmark, fresh recording state, complete tails and supported library backend selection. Resident Cohere ownership or verification caching would be an application lifecycle change requiring a concrete approved proposal. FFT/fused-attention/export work belongs inside the selected runtime. Making Uhm causal, training a denoiser for ASR fidelity, personal voice adaptation or checkpoint distillation requires new training/checkpoints; none is a configuration switch for the tested weights.
