# Automatic local spelling learning — 16 September 2026

> Historical evidence: names and commands below describe the recorded revision.
> For the current checkout, use the corresponding `vesper-*` package/executable
> and `VESPER_*` override names; legacy environment aliases remain supported.
> See [current naming and compatibility](../../docs/development.md#product-name-and-compatibility). No measurement has been requalified by renaming.

Phase A learns eligible scoped spellings from ordinary edits without a per-word Remember action. **All 54 automatic expected-output checks passed in 27 constructed episodes. The public replay activated no automatic memories and produced no held-out accuracy gain.** Automatic learning therefore remains an explicit setting, off by default. These results qualify implementation behavior, not long-term personal benefit.

The [implementation guide](../../docs/personalization/README.md) documents each component. [ADR-0016](../../docs/adrs/0016-automatic-local-spelling-learning.md) records approval and architecture. The local debug report at `.cache/benchmarks/results/2026-09-16-automatic-learning/debug-summary.json` preserves source/binary/fixture hashes, timing distributions, failures, public attribution and the cached recognizer's exact artifacts. Work remains tracked in [#34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34).

## Method and results

The chronological replay scores a prediction before providing its simulated edit. It uses the real application correction, admission, observation and SQLite code. Four separate empty stores run the same sequence: off; an oracle-assisted manual comparator; automatic edits; and automatic edits plus usage/retry evidence. No ASR, cleanup, training or cloud API runs inside this replay.

The [constructed fixture](fixtures/automatic-learning-episodes.json) covers one-edit technical spelling, two independent name corrections, repeated source/episode deduplication, later reuse, wrong project/language, quotes, numbers, dates, negation, continuation/repair cues and switching recognizers. Automatic modes each admitted two memories, improved five episode outputs and worsened none; both passed all 27 expected outputs. Retry evidence produced one possible-repair signal and no automatic correction label. The manual comparator worsened one deliberately changed-intent case, illustrating why its broader suggestion policy is not the automatic admission policy.

The public fixture combines **192 calibration recordings** (64 FLEURS US English, 64 FLEURS Hindi, 64 Svarah Indian English) followed by **48 held-out recordings** (24 FLEURS US English and 24 Hindi). It reuses the cached Nemotron Q8 hypotheses from the [prior native screen](2026-09-16-personalization.md) and [fresh held-out run](2026-09-16-memory-regressions.md). Each calibration reference supplies exactly one simulated ordinary edit; held-out references are used only to score. Every recording is unique by ID and audio hash. Synthetic timestamps are three minutes apart, so unrelated corpus sentences do not become fabricated retry episodes.

| Public replay mode             | Stored vocabulary | Candidates | Held-out word edits | Improved / worsened clips |
| ------------------------------ | ----------------: | ---------: | ------------------: | ------------------------: |
| Off                            |                 0 |          0 |                 181 |                     0 / 0 |
| Manual, oracle remember labels |                50 |          0 |                 175 |                     6 / 0 |
| Automatic edits                |                 0 |         14 |                 181 |                     0 / 0 |
| Automatic edits + usage/retry  |                 0 |         16 |                 181 |                     0 / 0 |

No store/replay execution failures occurred in either fixture. The public screen contains no natural longitudinal feedback, and many reference edits alter multiple spans or punctuation. The conservative single-spelling policy abstains; repeated independent confirmation is also absent. This is evidence of limited coverage, not demonstrated automatic improvement. No threshold was relaxed using held-out results. The manual comparator receives explicit oracle labels and a broader suggestion policy; it does not equalize approval effort or represent actual user behavior.

Here “word edits” means the core memory helper's lowercase whitespace-token Levenshtein distance after trimming edge punctuation. It is **not the normalized WER** in earlier `transcribe-eval` reports; do not compare their denominators or combine their scores. The JSON separates language totals and preserves every attempted row in aggregate counts. References and full hypotheses remain in the ignored local cache.

## Average latency

Apple M5 Pro, 64 GiB, macOS 26.6.2 (25G83), release build. The replay runs sequentially on CPU with no loaded speech/writing model. These are single-run component measurements with growing profiles/stores and ordinary OS caching.

| Automatic usage/retry mode                | Count |      Mean |       p50 |       p95 |       p99 |
| ----------------------------------------- | ----: | --------: | --------: | --------: | --------: |
| Constructed: apply remembered corrections |    27 | 0.0035 ms | 0.0032 ms | 0.0052 ms | 0.0072 ms |
| Constructed: edit feedback                |    11 | 0.2010 ms | 0.1922 ms | 0.2757 ms | 0.2757 ms |
| Public: apply remembered corrections      |   240 | 0.0098 ms | 0.0082 ms | 0.0206 ms | 0.0243 ms |
| Public: edit feedback                     |   192 | 0.3966 ms | 0.3796 ms | 0.6079 ms | 0.8419 ms |
| Public: usage/retry observation           |   240 | 0.4598 ms | 0.4623 ms | 0.7147 ms | 0.8801 ms |

Application timing covers only in-memory final correction. Mutation timing includes a fresh SQLite connection, transaction, commit and connection closure. Percentiles use nearest rank. Public application timing contains **zero active automatic memories**, so it is not a loaded-dictionary performance result; the constructed run covers two active entries. Maximum-quota sustained latency was not measured. The earlier warm-connection exploration is excluded from this table.

These figures exclude IPC/rendering, audio capture, ASR, cleanup and insertion. They are not total dictation latency or stop-to-final latency. Power, sustained thermal behavior, microphone queue behavior and end-to-end user timing were not measured in this component replay. The existing native latency evidence remains separately qualified in the prior reports.

## Verification and fixes

The full workspace suite passed **195 Rust tests** with 15 optional tests ignored; an additional review-ownership regression subsequently passed. Final frontend/tooling checks passed **70 Node tests**, zero Svelte/TypeScript errors or warnings, formatting, frontend build and strict Clippy. Native completion-report fixtures passed. The original restricted run's loopback-listener failure was retained; the complete workspace suite passed with local socket access.

Three installed-model tests also passed using the pinned public English recording: the real file pipeline and export provenance; Nemotron consecutive streaming/profile changes and cancellation/reset on CPU; and Qwen embedded-template formatting/role-token rejection. The generic smoke runner first failed its stream test because the default model folder lacked the explicit CPU runtime. The test passed with the already installed verified CPU artifact root. The file pipeline's automatic GPU attempt fell back to CPU under the sandbox; this is not GPU qualification. No weights were downloaded and no microphone was recorded.

Production Svelte components with mocked native commands passed single-click Copy with one edit commit, undo preserving authored text, failed edit/retry, learning off, forget, failed preference save/retry and a new result arriving before an old save fails. The last case retained the unfinished old text and showed the new result without a stale error. No JavaScript errors were observed. A captured UI screenshot was visually inspected; subsequent screenshot requests intermittently timed out in CDP, while DOM assertions completed. This fixture does not test native IPC or external applications.

Focused implementation fixes included retaining Unicode combining marks during spelling comparison, transaction rollback on injected write failure, permission epochs preventing stale callbacks after forgetting, keeping reviewed provenance separate from ordinary edits, preserving explicitly reviewed entries when automatic memory is forgotten, and preventing late draft callbacks from affecting another result. Ordinary drafts remain separate from supervised export targets.

## Reproduce

Portable controlled replay:

```sh
. ./scripts/dev-env.sh
cargo run --release --locked -p transcribe-data --example automatic_learning_replay -- \
  benchmarks/asr/fixtures/automatic-learning-episodes.json /tmp/automatic-learning-results.json
```

Prepare cached public feedback after following the linked native corpus protocols:

```sh
node scripts/prepare-automatic-learning-replay.mjs \
  .cache/benchmarks/personalization-2026-09-16/calibration.json \
  .cache/benchmarks/personalization-2026-09-16/results/calibration-nemotron.json \
  .cache/benchmarks/personalization-regressions-2026-09-16/fresh/holdout.json \
  .cache/benchmarks/personalization-regressions-2026-09-16/results/nemotron-fresh.json \
  /tmp/automatic-public-episodes.json
target/release/examples/automatic_learning_replay \
  /tmp/automatic-public-episodes.json /tmp/automatic-public-results.json
```

The preparer validates manifest hashes, model identity, complete successful upstream outputs and duplicate recording exclusion. It stops on upstream failures instead of dropping them. It reproduced the measured public fixture exactly as JSON; serialization can differ in its final newline. [FLEURS](https://huggingface.co/datasets/google/fleurs) / FLoRes and [Svarah](https://huggingface.co/datasets/ai4bharat/Svarah) attribution and pinned revisions are retained in the local debug report. Full replay and verification logs remain under `.cache/automatic-learning-replay/`.

These are local measurements and labelled simulations. Paper findings and official runtime capabilities remain in the [research synthesis](../../docs/research-automatic-personalization.md). Automatic semantic style learning, accent-model adaptation, external-document observation, training and general real-user improvement are not established or implemented by this phase.
