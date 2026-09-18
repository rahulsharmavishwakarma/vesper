# Audio frontend screening — 16 September 2026

Historical initial screen: the [hard regression follow-up](audio-regression-2026-09-16.md) adds 36 unused FLEURS recordings, more noise/level/boundary stresses, natural AMI filler annotations and longer replay. Its larger paired comparison does not reproduce the two-clip denoising benefit below. Preserve this screen's observations, but use the follow-up for the current conclusion.

**No candidate earned an automatic production default.** Recent Silero v6.2 is inexpensive but misses some very quiet clips. DPDFNet-8 helps the noisy read-speech examples while hurting synthetic controls. Uhm detects synthetic fillers but also marks hum and meaningful affirmation. Ear runs quickly but makes a confident language error.

The user’s final requirements are **model releases within the preceding year** and **up to 200 MB per candidate**, with smaller models welcome. The [release audit](audio-frontend-recency.json) distinguishes new checkpoints, old-checkpoint exports and documentation edits. Eligible tested checkpoints are Silero v6.2 (November 2025), DPDFNet-4/8 (December 2025) and Uhm (July 2026 core artifacts). Ear’s August 2026 deployment artifact retains older Whisper-tiny weights. Silero v4, GTCRN and CED are historical baselines only. Hush/Clear are recent research candidates, not executed or recommended here.

## Evidence and envelope

The local debug report at `.cache/benchmarks/results/audio-frontends-2026-09-16/debug-summary.json` contains 43 completed experiment reports, eight preserved first-pass failure reports, artifact/source/executable hashes, per-case outcomes, separate accuracy lanes and memory measurements. Additional paired signal checks are archived at `.cache/benchmarks/results/audio-frontend-alignment/debug-summary.json`. Full raw reports and PCM remain in the ignored benchmark cache; no personal audio was used.

- Apple M5 Pro, 18 logical CPUs, 64 GiB RAM; macOS 26.6.2 (25G83), AC power, low-power mode off. Our inference jobs ran sequentially. Other system load and CPU frequency were not controlled; no sustained thermal or power measurement.
- Native CPU: sherpa-onnx **1.13.8**, upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, existing application runtime build/patch identities retained in results. Frontends request one thread; existing ASR/writer helpers request up to eight. This is not a CPU instruction/operator-placement profile.
- Python CPU: ONNXRuntime **1.30.0** for Uhm, LiteRT **2.2.0** default CPU/XNNPACK interpreter for Ear, one inference thread; NumPy **2.5.3**, SoundFile **0.14.0**. No Core ML, GPU or NPU run. FP16 weight storage does not prove native FP16 arithmetic.
- Application HEAD `592e0084fe5cf486b4d56c575423e9a4f219bfb8` plus unstaged changes. Frozen native v3/v4 executables and embedded source hashes identify the actual runs. V4 adds recent-VAD context/tail handling and an explicit language guard; denoiser algorithms remain as in v3. The obsolete worktree-hash field in first-pass v1 reports is not current revision evidence; their embedded source/executable hashes are preserved.

The main corpus has **15 entries / 104.2565625 seconds**: four English and two Hindi FLEURS test clips, four task-authored synthetic English controls, four generated five-second non-speech signals, and one repeated first clip for reset testing. FLEURS revision is `70bb2e84b976b7e960aa89f1c648e09c59f894dd`, with CC-BY-4.0 attribution and hashes in [the manifest](../../benchmarks/asr/manifests/audio-frontends-screen.json). These are reused screening examples, not a newly untouched or speaker-disjoint qualification corpus.

Noise is a fixed white-noise/120 Hz hum mixture at 0 dB waveform-RMS SNR. “Quiet” multiplies speech samples by 0.03. Four separate noise-only signals are silence, white noise, hum and an 880 Hz tone. These are mathematical diagnostics, not natural noisy recordings or whispered speech. Two high-peak synthetic speech mixtures would clip at 0 dB; every configuration retains those preparation failures rather than changing their gain.

## CPU cost

RTF here means **frontend call/flush computation seconds divided by input-audio seconds**, excluding pacing sleeps, artifact verification, model loading, file I/O and ASR/writing. The first inference is included. Peak RSS is the **entire benchmark process**, not incremental model memory. A payload limit does not impose the same RAM limit.

| Candidate | Payload | Completed-file CPU measurement | Peak process RSS |
| --- | ---: | ---: | ---: |
| **Silero v6.2** | 1.29 MB | RTF **0.00205**; 214 ms over 104.26 s audio | 71.7 MB |
| **DPDFNet-4** | 11.71 MB | RTF **0.105** | 120.1 MB |
| **DPDFNet-8** | 14.62 MB | RTF **0.204** | 131.2 MB |
| **Uhm FP16, raw PCM** | 47.05 MB | Median **1,817 ms** per fixed 30 s model window | 1,467 MB |
| **Uhm FP16, SDK normalization** | Same artifact | Median **1,865 ms** per fixed window | 1,467 MB |
| **Ear, requested recent export** | 23.13 MB including sidecars | Median **73.3 ms** inference + **2.43 ms** preprocessing | 249.9 MB |

Uhm/Ear clips shorter than 30 seconds are padded to the fixed window; their call cost is not a per-second streaming cost. Short repeated calls, peak RSS and setup timings have different meanings. Historical baselines: Silero v4 FP32/INT8 RTF **0.00167/0.00296**, GTCRN **0.0182**, CED-mini/base **0.00280/0.0136**. The smaller v4 INT8 artifact was slower than FP32 on this CPU. These older checkpoints are outside the current shortlist.

## Does denoising improve the default two-stage path?

The [paired quality manifest](../../benchmarks/asr/manifests/audio-frontends-quality.json) contains 13 attempts per configuration. Input PCM hashes match between original, GTCRN, DPDFNet-4 and DPDFNet-8. ASR uses the actual pinned Cohere adapter, followed by the existing Qwen writer and fidelity check. No frontend parameter was tuned on the outputs.

The table reports **word edits / reference words**, excluding the reset repeat. Natural and synthetic lanes remain separate. The scorer counts some equivalent number formatting as word edits; these are not pure semantic-error counts.

| Input frontend | Clean English read speech, 2 clips | Clean synthetic controls, 4 clips | 0 dB English read speech, 2 clips | 0 dB synthetic controls, 2 valid clips |
| --- | ---: | ---: | ---: | ---: |
| Original audio | **5 / 33** | **8 / 52** | **14 / 33** | **4 / 26** |
| GTCRN — old baseline | 6 / 33 | 9 / 52 | 30 / 33 | 9 / 26 |
| DPDFNet-4 | 5 / 33 | 9 / 52 | 16 / 33 | 7 / 26 |
| DPDFNet-8 | **4 / 33** | 9 / 52 | **11 / 33** | 8 / 26 |

DPDFNet-8 merits further **conditional-noise** evaluation, not an always-on default: its gain on two noisy read-speech clips reverses on the synthetic controls. GTCRN changed “Keep the total” to “Eat the total” on a clean control. A small file or a better-sounding waveform is not evidence of preserved words. This agrees with the motivation of the [enhancement-artifact/ASR study](https://arxiv.org/abs/2404.14860v1), but does not reproduce that paper’s models or results.

Failures remain visible: **both Hindi cases are rejected by the current Cohere adapter** in every configuration. Thus there is no Hindi ASR accuracy result. Each 0 dB configuration additionally has the same two clipped-mixture failures. All four non-speech controls produce empty raw transcripts under all eight ASR configurations. The writer’s fidelity guard rejects the spoken-instruction control in all four clean runs and preserves raw text; rejected rewrites are not successful cleaned results. Raw/cleaned WER, CER, stage timings and failures are recorded separately.

## VAD, fillers and language routing

**Silero v6.2:** all ten unique clean speech clips have nonempty speech decisions and all four noise controls have none. Eligible 0 dB speech clips also have speech decisions. At 0.03× volume, **two English clips have no detected segment**. This is clip-level presence evidence, not annotated boundary recall or proof that every word survived. Audio remains unchanged in the test. Do not turn a negative VAD result into unconditional deletion.

**Uhm:** both preprocessing configurations emit candidate spans for synthetic “um” and “uh,” also for “hmm” in an affirmative sentence, and none in the meaningful “and” control. Both wrongly mark almost the entire hum-only clip as filler. The initial restricted speech-generation attempt produced four empty WAVs; those fixture failures are preserved, and a subsequent run used 19 verified nonempty cases. There are no natural annotated filler spans, so no natural precision/recall/F1 claim. Its actual ONNX output has **six classes**, despite a five-label sidecar/browser demo; this contract mismatch must be resolved in an adapter. The tested scalar postprocessor uses `1 − p(not_filler)`, not guessed class indexing or automatic audio removal.

**Ear:** its NumPy frontend matches the official SDK golden fixture to **1.76×10⁻⁵** maximum absolute error (author tolerance 0.002). Across ten unique short speech clips, nine top-language predictions are correct; one Hindi clip is confidently labelled Romanian. Three of four non-speech signals pass the SDK’s reliability heuristic. The flag is a host margin rule, not proof of speech or an unknown-language detector. The long-recording three-window selector was not tested. [Pinned author frontend](https://github.com/Desert-Ant-Labs/desert-ant-core/blob/2aed9c6d6459fc1843ab59553965035954e40199/Sources/Ear/Frontend.swift).

Ear identifies **language**; Uhm identifies **candidate fillers**. Neither recognizes persistent user identity or user intent. Their source-available terms differ from permissive backbone licenses. Internal benchmark results do not establish unrestricted redistribution or training rights.

## Streaming delivery and fixes validated

The native harness now handles empty buffered denoiser output correctly, pads offline input to the model hop, drains DPDFNet’s **640-sample** delayed tail and compensates its online signal offset. Before these corrections, GTCRN/DPDFNet failed sample-count checks on odd-length clips. First-pass failures remain archived. On an additional high-energy hard cut, completed/chunked denoiser outputs have **zero-sample best correlation lag**, cosine similarity above **0.99999999**, and matching finite sample counts. This verifies those signal contracts, not natural word-level tail accuracy.

Silero v6.2 uses 64 samples of initial context plus 512 new samples. The native API’s `Flush()` does not infer its buffered tail. V4 of the harness prefixes the context, feeds 576 EOS zeros, intersects returned segment intervals with the real audio and subtracts the context prefix from the native buffer coordinates. Regression tests ensure synthetic context/padding cannot escape the original recording’s timestamps. This reproduces the model-input contract; native segmentation policy still differs from the author's Python iterator.

Paced tests deliver **20 ms chunks against cumulative deadlines**, including a **54.92-second stitched speech recording**, then a fresh 12.64-second recording. Latest Silero’s p95 call time was **0.546 ms**, maximum delivery lateness **6.34 ms**, with finite bounded intervals and exact post-recording reset output. DPDFNet-8 and the older baselines also completed paced/reset checks; same-API reset outputs match exactly. Completed and online DPDF outputs differ only by small numerical effects after compensation.

This is **synchronous frontend replay**, not a live microphone queue or a live ASR pipeline. ASR/writing run after the complete waveform. First partial, word emission, application endpoint/stop-to-final, cancellation during a native call, actual queue drops, hours-long stability, thermals, mobile/x86 performance, natural overlap/music/singing and code-switching remain unqualified. No SOTA latency claim follows from this screen.

## Changes, reproducibility and next decision

Implemented development examples/probes, checksum-pinned preparation, explicit empty-reference noise metrics, failure-preserving summaries and regression tests. No production filtering, model ownership, default, training or speaker-enrollment change; no staging or commits.

Validation passed: **23 Rust tests**, Clippy on all evaluation targets with warnings denied, **four Python regression tests**, Ruff, official Ear frontend parity, all four noise-recipe hashes, paired-input equality, signal alignment and reset checks. The [research synthesis](../research-audio-quality.md) separates paper results, official capabilities, local observations and remaining hypotheses. [Issue #42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42) tracks completion and deferred qualification.

Proceed with recent Silero as a **conservative decision-only candidate** and DPDFNet-8 as a **conditional experiment**. Keep original-audio bypass. Uhm needs natural span/meaning evaluation and a better demonstrated edge runtime envelope; Ear needs routing fallback and longer/mixed-language tests. Latest-export parity, natural held-out evaluation and production architecture approval remain separate gates. Efficient FFT/native buffer work belongs inside the runtime; denoiser retraining and personal speaker extraction require checkpoint/data work. Neither is enabled by adding these artifacts.
