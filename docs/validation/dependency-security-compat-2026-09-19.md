# Dependency security compatibility — 19 September 2026

This change repairs two CI dependency findings without adding Python to the
application. GLib receives a maintained upstream source fix for Tauri's Linux
GTK3 graph. Transformers 5.10.1 is used only by the isolated VibeVoice research
probe. Application model adapters, defaults and recording behavior are unchanged.

## GLib

The official GLib 0.18.5 archive has SHA-256
`233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`.
All 121 archive files are retained. Only the upstream two-line mutable-pointer
fix in `src/variant_iter.rs` changes. [Provenance](../../patches/glib/provenance.json)
records every resulting file hash.

- The optimized isolated crate compiles on Linux and passes both iterator tests,
  covering forward/backward iteration, skips, last, Unicode, empty strings and
  exhaustion. Local container: `rust:1.98.1-bookworm`, Linux arm64, system GLib.
- All 13 audit guard tests pass. They reject altered source/provenance, symlinked
  source, unpatched duplicates, redirected Cargo resolution, unexpected audit-lock
  changes and regression dependencies outside the audited graph.
- A real cargo-audit run with a synthetic second GLib advisory fails as required.
  Only verified repaired `RUSTSEC-2024-0429` is excepted. The temporary audit lock
  preserves registry matching for other GLib advisories; the build lock is unchanged.
- The current RustSec scan passes with six existing unmaintained-package warnings.
- Workspace target-tree checks show no GLib in the Apple-silicon macOS graph and
  the patched GLib in the Linux GTK3 graph. This does not qualify a Linux app release.

The [maintenance procedure](../../patches/glib/README.md) describes the source
guard, audit policy and removal conditions. The two upstream files with nonstandard end-of-file whitespace are excluded only
from that formatter, and remain checksum-verified.

## VibeVoice compatibility envelope

The model remains `microsoft/VibeVoice-ASR-Streaming-1.5B`, checkpoint
`4262d23d8a539a6530cf64fbd0b1751ef9a30853`; publisher source
`1541f590c7099820f10ea012f48d2399282df69f`. The
[artifact manifest](../../benchmarks/asr/vibevoice-streaming-artifacts.json)
contains the unchanged artifact hashes. The maintained
[patch and provenance](../../patches/vibevoice/README.md) identify the local
Transformers 5 port; Microsoft still pins its upstream source below version 5.

The comparison uses the existing 43 completed-audio and five paced-delivery
cases, 682.9034375 seconds in total, in the same order. Cases include English,
French and Hindi FLEURS, noise and synthetic controls, hotwords, AMI meeting
speech, quiet input, seed/reset controls and a 180-second repeated-speech soak.
These are existing small regression plans, with repeated/derived controls,
not 48 independent natural recordings or a new model qualification corpus.
The [original screen](vibevoice-2026-09-18.md) documents corpus preparation,
licenses and its separate historical environment.

Local execution: Apple M5 Pro, 64 GiB RAM, macOS 27.0 (26A428), CPU FP32/SDPA,
eight Torch intra-op threads and one inter-op thread. Both compared environments
use Torch 2.13.0, Diffusers 0.38.0, NumPy 2.2.6, Safetensors 0.8.0 and Tokenizers
0.22.2. Baseline Transformers is 4.57.6; candidate is 5.10.1 with Hub 1.5.0.
The probe uses greedy decoding, a 256-token chunk cap and a 300-second per-case
wall timeout. Each process loads one resident model and starts a fresh stream
state per case. Input is 24 kHz; chunk advance is 70,400 samples with 12,800
samples of right context and final-window zero padding. No endpoint detector,
microphone queue or application writer participates.

## Compatibility checks

- The Python dependency audit passes with no advisory exceptions.
- Vocabulary, special/added tokens, token IDs, offsets, decoding and chat templates
  match the 4.57.6 fixture: 20 synthetic text controls and four chat cases. An
  additional local comparison covers all 48 public-plan references.
- Small tied and untied synthetic checkpoints survive save/load with every tensor
  unchanged. Publisher and built-in acoustic configurations dispatch separately.
- The actual probe checks all 1,177 downloaded checkpoint tensors, missing/unexpected
  keys, shared embedding identity and the configured acoustic sampling buffer.
- The first full port replay exposed a Hindi token-cap failure despite correct
  checkpoint tensors. The Transformers 5 loader materializes non-persistent buffers
  from meta storage; the publisher's initializer did not restore `fix_std=0.5`.
  The patch now initializes this buffer explicitly. The affected Hindi case then
  finishes and exactly matches baseline output. The failed run remains retained.

The final replay completes **48/48 cases with no failures or token-cap hits**.
Every decoded chunk, generated-token count, cache length and complete raw output
matches the 4.57.6 baseline exactly. All audio is covered. The three seed/reset
controls reproduce the original output from a fresh prompt cache, including the
reset after the 180-second paced soak. No cases were dropped from the comparison.

The canonical scorer consequently gives identical scores for both runtimes.
Representative existing lanes remain poor enough to rule out a new quality
recommendation; matching the baseline establishes compatibility only.

| Existing lane | Word edits/reference words (WER) | Grapheme edits/reference units (CER) |
| --- | --- | --- |
| English natural (24 cases) | 70/610 (11.48%) | 197/3002 (6.56%) |
| English controls (4 cases) | 10/52 (19.23%) | 51/255 (20.00%) |
| French (2 cases) | 16/46 (34.78%) | 34/229 (14.85%) |
| Hindi (2 cases) | 63/64 (98.44%) | 186/140 (132.86%) |
| Meeting (1 offline case) | 40/134 (29.85%) | 122/485 (25.15%) |
| Quiet control (1 case) | 12/12 (100.00%) | 49/57 (85.96%) |

No inference speedup or improved model quality is claimed. The earlier published
quality/latency limitations continue to apply. The failed first port replay
remains evidence of the fixed initialization regression, not a discarded success.

## Reproduction and limits

Use the locked commands in [development](../development.md#python-tooling-policy).
Prepare the existing public plans, concatenate offline cases followed by paced
cases, and run the two environments sequentially with identical plan/artifact
hashes and settings. The baseline runner is `scripts/probe-vibevoice.py` at
`7a596c1af9a61038dbc6dc8ae1d71e42df27d7f8`, SHA-256
`ab009f224d8d461add272270b779b7884110341e11b51106eaa3e06287924fa4`.
The candidate uses this change's probe and checksum-verified source patch.
Record the runner, patch, plan and raw-report hashes rather than implying that
the base revision already contains these uncommitted changes.

Raw public-audio reports remain in ignored local evidence. Do not add private
recordings, raw transcript text or downloaded weights to source control.
Canonical WER and grapheme error rate use the existing release-built
`transcribe-eval` `score_json` helper. Failed cases must remain counted.

Timing is observational: other validation processes ran during these replays,
so these are not controlled speed or resource comparisons. Per-case first-partial,
stop-to-final and compute timings plus process RSS are retained locally.
The candidate's `loadMs` includes full checkpoint-tensor verification; the old
runner's loading measurement excludes that additional work and is not comparable.
Observed process peak RSS was 12.01 GiB for the baseline and 16.16 GiB for the
candidate, including its additional verification work. These are process peaks,
not steady-state application memory measurements; the higher candidate peak is
retained rather than presenting the port as a memory improvement.
Word-aligned emission, endpoint latency, power, sustained thermals, microphone queue drops,
application cancellation/insertion and independent meaning-preservation review
were not measured. End-of-stream reason remains indistinguishable between the
publisher API's chunk-end and EOS results. Reset checks cover explicit fresh
stream state, not an application cancellation interface.

Local Python/tooling, formatting/lint/types, 97 Node tests and workflow checks
pass. Hosted CI and native macOS application checks must run after publication;
this local report does not claim that PR #69 is green or that a release exists.

Before committing the integrated changes on 20 September, the source checks,
strict Rust audit, locked VibeVoice Python audit, formatting/lint/types, Node
tests, Python regression suites and workflow checks were rerun successfully on
the current main checkout. The Rust audit retains the same six maintenance
warnings. This integration check does not repeat the model replay or replace
the outstanding hosted CI and native qualification.

## Evidence fingerprints

Base application revision is recorded above; local edits are identified by these
content hashes. Reports stay in the ignored evidence directory.

| Artifact | SHA-256 |
| --- | --- |
| Baseline full report | `6d243d2c68ab7f22d382a28a73b6a14ba46422c0da26e23d4ebcb0f020df7d22` |
| Initial failed port report | `72f0cfa3d7890b6f1aea4e35ffc10be7dbe794f6043ac386c371efee46b2b9ee` |
| Final full report | `05e7fb71f654bd9aaefa054709f3840646a1012cea19af7a9fcf9841220c8001` |
| Final runner | `5b25af78129ff40250cfaaa727f27624c675d7eeeedd7e3378cc4107fb5677c7` |
| Final source patch | `f8f5753ed60f9d307d73fcc3fae7ca9317b5ecc39c9cd52fb097f589544e70c5` |
| Executed combined plan | `f81ba7696e8e70483e06d01abdaa65b10b17cd7113819fe3ba8830e7a03a3360` |
| Canonical scorer executable | `a89d1bfad95d7904d81ef89da7161f7504a246d6e7382c9f520cbd2c44f6a08a` |
