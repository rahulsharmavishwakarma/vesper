# Shared Whisper adapter qualification — 18 September 2026

The Small/Turbo architecture adapter, verified INT8 installation and shared selected-resident Sherpa ownership are implemented under [ADR-0018](../adrs/0018-shared-whisper-adapter.md), with specific user approval recorded in [issue #45](https://github.com/rahulsharmavishwakarma/transcribe/issues/45). Product presets remain unavailable: noise and repeated-speech quality gates fail. Cohere plus Qwen remains the default; Kyutai remains removed.

This is the application `InferenceManager` path, separate from the earlier [development probe](whisper-2026-09-18.md). The local debug report at `.cache/benchmarks/results/whisper-adapter-2026-09-18/debug-summary.json` retains source/binary/artifact hashes, process failures, missing cases, stage errors, confidence/completion records and transcript hashes without publishing transcripts or audio. Published model/paper results are described separately in [the research review](../research-whisper.md).

## Executed contract

One Whisper adapter serves multilingual Small and Turbo. Checkpoint differences select pinned artifacts, not separate implementations. Cohere and Whisper share verified resident ownership, identity/invalidation, worker dispatch and fresh streams; all supported generative paths retain the shared segment-boundary implementation. Whisper gets contiguous segments of at most 29 seconds; Cohere keeps ten seconds. Neither overlaps nor drops input samples. Every successful segment requires actual native EOS and untruncated input; failed/cancelled recordings do not publish a successful prefix.

The native patch removes the six-tokens-per-second heuristic while retaining half-text-context and fixed KV bounds. Confidence comes from native logits and token metadata, with separate score/language stream options. No new attention kernel, KV representation, quantized format, backend, VAD model or training is implemented. The official 0.6 no-speech / −1 average-log-probability joint rule remains fixed: low confidence alone does not delete speech. The rule is insufficient for these INT8 artifacts on the noise cases below.

## Envelope and reproducibility

- Hardware: Apple M5 Pro, 18 logical CPUs, 64 GiB RAM; macOS 27.0 (26A428). This is a development laptop, not a low-memory edge device.
- Runtime: CPU, eight native graph threads; `OMP_NUM_THREADS`, `OPENBLAS_NUM_THREADS`, `VECLIB_MAXIMUM_THREADS` and `MKL_NUM_THREADS` set to one. Qwen3.5-0.8B writing uses Q4_0 on CPU; experimental prefix cache disabled.
- Application HEAD: `592e0084fe5cf486b4d56c575423e9a4f219bfb8` plus unstaged implementation. The JSON records exact frozen source/executable hashes, because HEAD alone does not identify this build.
- Sherpa 1.13.8 upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`; native build `247e90de15e1772d0122836444a0fc6b1f92f85b32555f1eb3cabaa1c4cbbf69`; patch SHA-256 `b6bfad9330296d2c56309e3dd10d2848820d89cd18e376f90686da2b6595237a`. Static-library hashes are in native provenance.
- Small export revision `8f3c18b358db4d1f2fc1eae49d75cd20989e4309`, 375,485,327 bytes. Turbo `2ca6ff69fc878651b770880507669577ac41c2ff`, 1,036,613,791 bytes. Each encoder, decoder and token file was checked by size and SHA-256 against `models/artifacts.json` before execution.
- Completed-file mode, greedy transcription, tail padding 1000 frames. No live Whisper streaming, rolling-window algorithm, endpointing or cross-recording generated-KV reuse. First-partial, word-emission, partial stability, queue-drop and microphone stop-to-final measurements are not applicable to this offline experiment.
- Power source was recorded per process (AC); OS caches were not flushed. CPU package power, operator placement, thermal state, sustained battery behavior and accelerator/edge performance were not measured. RSS is process memory, not artifact size or VRAM.

Commands and fixture requirements are documented in [ASR qualification](../../benchmarks/asr/README.md#shared-whisper-adapter-qualification). The local result root is `.cache/benchmarks/results/whisper-adapter-2026-09-18`, with frozen binaries and separate main, repeated-English, automatic-language, meeting and lifecycle process ledgers. Builds finished before the serialized qualification stages. External compilation and desktop activity observed by the runner remain in the evidence; their timings are not quietly promoted to an uncontended comparison.

The evaluation planned 317 sample executions: 158 in the main suite, 90 in the repeated English pass, nine in the meeting/reset lane and 60 in the English-auto lane. There are 309 recorded rows; eight explicit-Hindi samples were rejected at process preflight. The 300 lifecycle cycles are additional mixed operations, not 300 independent speech recordings.

## Accuracy and meaning

The main English lane has 24 natural FLEURS US-English test recordings (610 normalized reference words), four synthetic meaning controls and two reset repeats. Two processes reverse model order across 15-case blocks. The entire English lane was repeated with the same frozen binary; there were no changed raw/cleaned transcript hashes, scores or failure outcomes. Repetition demonstrates regression stability, not additional independent samples. These clips were already observed in the earlier screen; no model, padding or confidence threshold was tuned on them during this implementation.

| Model | Natural English raw WER | Natural English after accepted cleanup | Environmental noise with false text | Quiet −30 dB control WER |
| --- | ---: | ---: | ---: | ---: |
| Cohere | 59/610 = 9.67% | 9.67% | 0/3 | 12/12 = 100% |
| Whisper Small | 45/610 = 7.38% | 7.38% | 2/3 | 4/12 = 33.33% |
| Whisper Turbo | 30/610 = 4.92% | 4.92% | 3/3 | 4/12 = 33.33% |

The three environmental clips are five seconds each from MS-SNSD test recordings, not a broad noise evaluation. Small inserts two words on two clips; Turbo inserts one on all three. The other Small noise clip is suppressed by the joint score rule. Scores and successful EOS remain recorded. Quiet speech is one attenuated public recording plus a repeated control: preserving nonempty text is not perfect recognition, and none of these results establishes a general gain/noise policy. Cohere's blank quiet output is retained as a 100% deletion outcome rather than counted as success in quality terms.

Manual inspection of the four synthetic controls found that all three models retained the negation, amount distinction, appointment values and self-correction. Normalized WER still penalizes some numeric formatting. The spoken-instruction control caused a formatting-only writing fidelity rejection for every model, preserving raw ASR; Small also misspelled a word in that raw control. These are three cleanup failures per English pass, not ASR decoder failures. Small had one additional cleanup rejection on a noise clip. Raw and written outcomes are not combined into an artificially complete cleaned-WER aggregate.

French explicit/automatic modes agree on these four cases per model. On the two natural recordings (46 words), Small has 7/46 = 15.22% WER and Turbo 4/46 = 8.70%. Repeating the first utterance three times exposes large deletions: Small has 55/78 errors both with and without leading padding; Turbo has 55/78 and 54/78. Native EOS and complete sample delivery do not prove semantic coverage. These synthetic stress cases are separate from the natural-French aggregate.

A separate automatic-English pass executes all 30 English cases for each Whisper variant. Its raw hypotheses match explicit-English output on every case, with the same 24-recording natural WER and the same one cleanup rejection per model. This is a compatibility check on the existing corpus, not a new independent accuracy sample.

Explicit Hindi is rejected before inference: two failed processes account for eight planned but unexecuted sample rows. Automatic Hindi produces eight failed rows, including Small's explicit model-token-limit rejection on one padded repetition; other outcomes reject the detected unsupported language. No Hindi WER recovery is claimed. Code-switching and singing remain hard-filter exclusions; no singing corpus was executed.

## Natural long audio and boundary coverage

A separate 45-second AMI ES2004a mixed-speaker, far-field excerpt (meeting time 600–645 seconds) uses the same 24 kHz PCM preparation and word-start-ordered reference as the earlier VibeVoice comparison, then the common evaluator's 16 kHz resampling. The reference includes overlaps, disfluencies and speaker backchannels; it is not a clean single-speaker dictation lane. Each process runs English A → meeting → A. All three return matching A before/after and successful EOS for every segment.

| Model | AMI WER, 134 words | Resident ASR time for 45 s | RTF | Native segments |
| --- | ---: | ---: | ---: | ---: |
| Cohere | 46/134 = 34.33% | 2.285 s | 0.0508 | 6 |
| Whisper Small | 49/134 = 36.57% | 3.018 s | 0.0671 | 2 |
| Whisper Turbo | 44/134 = 32.84% | 4.825 s | 0.1072 | 2 |

All three meeting processes had no detected competing work. This is one excerpt per model and no writing stage; it provides a narrow comparable measurement, not a broad latency distribution or meeting-quality qualification.

The 180-second stitched-speech soak delivers all 2,880,000 samples: 20 Cohere segments and seven for each Whisper variant, followed by an identical A reset. It is a synthetic coverage test, not native streaming or a natural continuous monologue. Its historical manifest has an empty reference and a `noise` scenario despite containing speech. The publication explicitly excludes those invalid non-speech scores, retains the original report hash and execution evidence, and does not invent a WER reference.

## Latency and resident ownership

`asrMs` measures the common manager call after audio loading/resampling; the first request includes integrity verification/session construction. Subsequent resident requests reuse the selected model. `asrMs + cleanupMs` measures those two calls only, excluding capture, endpoint detection, file loading, insertion and UI latency. It is not stop-to-final. Model load/verification metrics are lifetime measurements, identified by resident-load ordinal, not costs paid on every request.

The first broad English pass detected competing compilation/desktop work in three of six processes; the repeated pass detected background Metal compilation in two of six. Keep these failures of measurement isolation visible. The JSON exposes distributions and their `latencyUsable` flags, but there is no uncontended all-model English ranking. Turbo's uncontended repeated pass has 28 resident ASR requests, p50 1.123 s and p95 2.424 s; its 27 successful two-stage requests have p50 1.509 s and p95 3.017 s. The failed cleanup request remains in the failure aggregate and is excluded only from the successful two-stage timing distribution. None of these numbers demonstrates SOTA CPU/edge latency.

Cold English requests recorded roughly 0.55 seconds of verification plus session construction for Small and 0.84–0.85 seconds for Turbo in this envelope. The lifecycle tests establish that these stages are not repeated during stable resident reuse; those components are not a paired end-to-end speedup experiment.

## Lifecycle and checks

All three 100-cycle installed-model tests passed (300 mixed cycles total, plus cold/reset/invalidation checks):

| Model | Resident ASR-only RSS | Maximum sampled RSS from cycle 10 onward | Growth from cycle 10 | Result |
| --- | ---: | ---: | ---: | --- |
| Cohere | 4,315.0 MiB | 5,564.2 MiB | 13.3 MiB | Pass |
| Whisper Small | 1,210.0 MiB | 2,672.1 MiB | 0.031 MiB | Pass |
| Whisper Turbo | 2,157.6 MiB | 3,813.0 MiB | 5.8 MiB | Pass |

The larger mixed-cycle RSS includes the resident Qwen writing model. Peak process RSS from `/usr/bin/time` is recorded separately. Turbo and Cohere lifecycle processes observed external contention; use them for their asserted state/memory checks, not an uncontended latency comparison.

Each installed-model test executes 100 mixed cycles after fresh cold baselines. Coverage includes A/B/A, exact-zero input without a cold model load, a natural meeting over 29 seconds, French/English and ASR switches, writing calls, pre-cancellation, timed cancellation, cancellation from progress callbacks, unload/shutdown, and copied-artifact corruption/removal/replacement/change-during-request. Installed original artifacts are never mutated. The shared resident-load ordinal stays one through the 100-cycle loop; model/language/unload/invalidation changes force new loads. A native oversized-Whisper rejection and a following exact A verify early-error reset.

RSS growth compares cycle 10 to the maximum later checkpoint against the predefined 128 MiB ceiling. These mixed loops are not 100 hours or a low-memory-edge qualification. RSS after unload/shutdown need not fall to initial process RSS because allocators retain memory and the sequence reloads between observations. Timed cancellation signals at 20 ms often land during waveform acceptance before native decode; those return times do not prove mid-kernel interruption. Native calls remain synchronous, and arbitrary injected ONNX execution faults, thermal behavior, other operating systems and physical microphone delivery remain unqualified.

Final checks passed:

- 113 Rust unit/CLI tests across core, models, inference and evaluation; 14 installed-model tests are ignored by default, with the three shared lifecycle tests executed separately above.
- Native report/score/budget helper tests, including invalid logits and EOS-versus-cap precedence; three publication regression tests covering stage classification, invalid soak scoring and transcript/path removal.
- Strict Clippy for core/models/inference/evaluation and their targets; complete Rust workspace check, including the desktop crate; Rust formatting check.
- Svelte check: zero errors/warnings. All 80 JavaScript/tool tests passed. Ruff passed for the changed qualification/publication scripts.

The implementation and documentation remain unstaged and uncommitted. There is no changed default, automatic promotion or claim of qualified Whisper production availability.

## Decision

Keep one architecture adapter for both Whisper checkpoints and reuse common lifecycle/segmentation infrastructure. Keep the pinned CPU implementation available for evaluation and installation, but withhold product selection and recommendation. Better read-English accuracy does not outweigh failed noise/repetition gates, limited language evidence and the absence of a broad isolated speed comparison. Any noise-policy experiment needs separate development and held-out recordings with quiet-speech false-rejection checks; do not tune on these observed clips or claim that a new VAD automatically solves omissions. A new frontend subsystem, runtime backend or live Whisper algorithm requires its own bounded proposal under the project rules.
