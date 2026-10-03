# Model presets

For a development model directory, use `VESPER_MODEL_DIR`. Earlier environment names remain
[compatibility fallbacks](development.md#product-name-and-compatibility), without
renaming model IDs or moving downloaded artifacts.

The Models menus include eight ASR choices and six cleanup choices. Qwen3-ASR and Cohere use dedicated adapters described in [offline ASR](offline-asr.md).
[LFM2.5 Audio 1.5B](lfm-audio.md) adds English-only completed-audio recognition on CPU with a pinned Q4 backbone/projector pair (915,262,016 bytes).
The exact application-language choices, delivery modes, and benchmark status for every ASR option are summarized in [Language support and model recommendations](language-support-and-recommendations.md).
The original built-ins remain available; the other additional entries are pinned profiles from
[models/presets.json](../models/presets.json). They use the existing profile,
settings, explicit-download, hash-verification, and inference paths. Selecting
an entry changes only the settings draft. **Save & download** saves it before
requesting that model's files; **Save changes** alone does not download speech or
writing weights. Supporting audio packages are managed automatically once those
required models are ready; see [automatic audio setup](user-guide.md#experimental-audio-processing).

The menus are the user-facing catalog; there is no JSON profile import section.
Previously saved custom profiles remain visible in their respective menus.
Writing presets declare their supported languages; the language menu checks both
speech and enabled writing models. Older writing profiles without that metadata
need their preset reselected or polishing disabled before saving. They remain
readable, and runtime writing falls back to the transcript with a warning until
repaired. See [writing compatibility and primary sources](language-support-and-recommendations.md#writing-language-compatibility).
Additional supported models should be added through the catalog process below.

## Live transcription support

Live means words emitted during microphone capture, before Stop, in Dictate or
the floating recorder. This table describes the adapters wired into Vesper, not
every capability of an upstream architecture or checkpoint.

| Speech model or preset | Words while speaking | Current application path |
| --- | --- | --- |
| NVIDIA Nemotron 3.5 ASR Streaming 0.6B | Yes | Incremental microphone chunks and revisable partial events |
| Cohere Transcribe (default) | Yes | Buffered drafts; authoritative completed-audio pass after Stop |
| NVIDIA Parakeet TDT 0.6B v3 | Yes | Buffered drafts on CPU or qualified MLX |
| Qwen3-ASR 0.6B | Yes | Native MLX drafts for English/Metal; buffered CPU drafts; completed-audio final |
| Liquid LFM2.5 Audio 1.5B | Yes | Buffered drafts through its current CPU/GPU runtime |
| Parakeet TDT v2, Parakeet Unified, FastConformer European presets | Yes | Buffered drafts with the selected transducer export |
| Whisper Small / Turbo (not in the desktop picker) | Yes | Buffered drafts; catalog quality restrictions remain |
| Imported custom ASR profiles | Yes | Buffered drafts for compatible validated transducer profiles |

The common live interface selects native sessions or buffered previews after
resolving the actual backend. Only Nemotron and the qualified Qwen MLX path
advertise native input streaming. Other speech adapters retain their offline
model contract behind bounded live previews. Every draft remains revisable;
writing and insertion operate on the final result. Update frequency depends on
model speed and device resources, and very short recordings may finish before a
nonempty draft is available.

See [live adapter behavior, resource policy and finalization](live-transcription.md)
for implementation and validation boundaries. The [source review](research-streaming-inference-papers.md#native-live-input-feasibility-by-adapter)
retains the earlier native-capability investigation as dated evidence.

### Historical baseline check — 26 September 2026

Before the all-model live implementation: source revision `6f90343`, debug `vesper-eval`, Apple M5 Pro (18 logical CPUs,
64 GiB), macOS 27.0 (26A428), AC attached with no reported thermal warning before
the run. This was a functional replay on a shared host, with no concurrent
compilation. Nemotron Q8_0 initialized Metal; native ASR used four graph threads,
Silero/DPDFNet8 used one CPU thread, gain and writing cleanup were off. Runtime
revision was `4f9676226f667d14608487df744f375db87127f8`; all model/frontend pins
are in [the artifact manifest](../models/artifacts.json). The encoder used its
artifact defaults (56 left-context frames, 3 right-context frames, nominal 320 ms
advance), without endpoint or decoder changes.

Two public FLEURS English clean clips, `16355757648693617724` and
`13297690853135603331`, came from the existing
[frontend integration manifest](../benchmarks/asr/manifests/frontend-integration-2026-09-18/streaming.json).
Their durations were 7.92 and 7.68 seconds. Replay used `--streaming --realtime
--language en --vad --denoiser dpdfnet8 --chunk-ms 20`; model startup preceded
paced audio delivery, matching the application's load-before-Listening order.

| Request | First nonempty partial from streaming start | Stop-to-final ASR | Events, including repeated drafts and final |
| --- | ---: | ---: | ---: |
| Cold recognizer | 15,290.95 ms, including initial loading | 63.40 ms | 331 |
| Resident recognizer | 1,353.41 ms | 61.69 ms | 319 |

Both completed without frontend fallback, preserved sample counts and finalized
the audio frontend. The resident draft arrived before the 7.68-second audio
delivery ended. The cold first-partial time includes startup and must not be
reported as microphone-to-word latency. Four word edits occurred across 40
reference words; this small reused sample is not an accuracy qualification.
Whole-process peak RSS was 1,197,375,488 bytes. The evaluator binary SHA-256 was
`917ee593d44805a4891d4daec19f86d4b84eaa31a185f1963374bd4d312289bb`.

All six nonstreaming built-in models and the three additional ASR presets
rejected a streaming request before model loading, as their contracts require.
A separate CPU replay attempt stopped at readiness checking: this model folder
contained the Metal runtime package, not the CPU package. That failure is
retained; no CPU streaming result is claimed and installed packages were not
replaced for this check.

Three adapter-registry tests, three live-state reducer tests and all three
browser suites passed, including mocked live updates and recording isolation.
Native microphone-to-visible latency, GPU operator placement, VRAM, power draw,
continuous thermal behavior and broader accent/meaning preservation were not
qualified by this replay. See [performance evidence](inference-performance.md)
for separately scoped measurements. No application model choice or runtime
behavior changed during this investigation.

## Choosing the default from benchmarks

Default selection follows comparable local application benchmarks, rather than
release date or a publisher leaderboard. First require an executable, pinned
adapter compatible with the requested language, recording mode, backend and
duration. Then evaluate recognition and meaning preservation, noise-only false
text, failures, latency and memory on the same held-out inputs and hardware.
Keep cold/resident, streaming/completed, precision and preprocessing conditions
separate. A faster or more accurate candidate that fails required quality gates
does not become the default. The existing [recommendation evaluator](../benchmarks/asr/README.md#compare-and-recommend)
supports explicit accuracy, latency and balanced objectives; its output is
development evidence and does not automatically change saved settings.

Cohere Transcribe with optional Qwen cleanup remains the current
completed-recording default. This is a provisional product choice, not a claim
that it wins every benchmark. In the [paired Whisper qualification](validation/whisper-adapter-2026-09-18.md),
Turbo has lower clean-English WER, but noise and repeated-speech gates fail.
The earlier Nemotron/Qwen comparison supports Nemotron for its named balanced
CPU workloads; it does not rank Cohere, Whisper and every catalog entry in one
comparable envelope. A new release or one favorable signal condition is
insufficient evidence to replace the default. There is also no all-writer
comparison establishing Qwen cleanup as a universal winner.

The [25 September fresh public holdout](inference-performance.md) adds 448
completed-audio evaluations on 32 English and 32 Hindi recordings. Qwen MLX has
the lowest observed English WER (3.15%); Parakeet MLX is fastest (48.2 ms mean
ASR for subsequent requests). Nemotron Metal has lower Hindi WER than Qwen CPU
on that sample (14.42% versus 25.17%). Cohere retains one incomplete decode out
of 32 English requests. These small read-speech results identify promising
configurations; they do not replace the noise, meaning, language/mode or
native-device gates. The default remains provisional and unchanged.

Qualify optional gain and denoising separately for each model and artifact.
One model's regression does not invalidate another model's measured improvement.
The [automatic-level results](validation/denoising-followup-2026-09-19.md#automatic-level-implementation-and-qualification)
remain mixed and limited. The desktop now uses an owner-selected fixed setup:
DPDFNet8 suppression plus Silero advisory timing; automatic gain remains disabled.
This is a product decision, not a passed universal quality gate or an adaptive
per-ASR policy. Benchmark configurations still explicitly select original audio,
DPDFNet4/8 and gain conditions; their defaults and historical results are unchanged.
Only ASR and writing models have user-facing model selectors. Missing supporting
packages download when the required selected models and relevant native runtime
are ready. Stored files are verified and reused automatically; missing/corrupt
files are repaired. Failures/cancellation retry with backoff or network reconnection.
The existing download operation retains pinned hashes and serializes with inference.
The speaker runtime still requires a separate local build; unavailable runtime
support does not trigger an unusable weights download.

## Additional choices

| Model and original source | Languages declared upstream | File precision | Download bytes |
| --- | --- | --- | ---: |
| [Parakeet TDT 0.6B v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) | English | INT8 | 661,190,513 |
| [Parakeet Unified English 0.6B](https://huggingface.co/nvidia/parakeet-unified-en-0.6b), offline export | English | INT8 | 663,043,117 |
| [FastConformer European 114M](https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/stt_multilingual_fastconformer_hybrid_large_pc_blend_eu), transducer branch | English, German, Spanish, French | FP32 | 485,282,823 |
| [Qwen2.5 0.5B Instruct](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF) | Multilingual | Q4_K_M | 491,400,032 |
| [Qwen2.5 1.5B Instruct](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF) | Multilingual | Q4_K_M | 1,117,320,736 |
| [SmolLM2 360M Instruct](https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct-GGUF) | English | Q8_0 | 386,404,992 |
| [SmolLM2 1.7B Instruct](https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF) | English | Q4_K_M | 1,055,609,536 |
| [Gemma 3 1B IT](https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF) | Multilingual | Q4_K_M | 806,058,240 |

Sizes describe downloaded files, not runtime RAM. Precision is fixed by each
preset's files. In particular, SmolLM2 360M uses its author's Q8_0 artifact; the
menu does not relabel it as Q4. The original Qwen3.5 selection still offers its
two existing Q4_0/Q8_0 downloads. Additional precisions require a validated
catalog entry.

Parakeet v2 uses CC-BY-4.0; Unified uses the NVIDIA Open Model License;
FastConformer uses NVIDIA NGC Terms of Use. The added Qwen2.5 and SmolLM2 models
use Apache 2.0; Gemma uses Gemma terms. Profiles retain these declarations in
`models/presets.json`. The original sources above
provide model terms and language claims. These profile presets do not add Hindi support. Nemotron and the dedicated Qwen3-ASR adapter are the built-in Hindi options.

## Compatibility evidence — 14 September 2026

The ASR presets match the four-file offline NeMo transducer contract documented
by [sherpa-onnx](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/offline-transducer/nemo-transducer-models.html).
The exact packaged version is 1.13.8, commit
`11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`. Its
[FastConformer export and test script](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/scripts/nemo/fast-conformer-hybrid-transducer-ctc/run-transducer-non-streaming.sh)
maps the four-language checkpoint to the selected transducer files. The
[Unified export workflow](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/.github/workflows/export-nemo-parakeet-unified-en-0.6b.yaml)
provides the selected non-streaming export. These presets run through the app's
existing CPU adapter and finalize completed recordings.

Cleanup uses llama-cpp-2/llama-cpp-sys-2 0.1.156, bundling llama.cpp revision
`e79e4bf660e19f2ad851e06c6913f7a8c5852621`. The
[architecture table](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/src/llama-arch.cpp)
and [template formatter](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/src/llama-chat.cpp)
support the Qwen2, Llama/SmolLM2, and Gemma3 paths used here. A small probe compiled
that exact formatter and confirmed that the Hub-reported templates retain both
the synthetic system instruction and user text. This checks prompt formatting;
it does not load or tokenize the downloaded GGUFs.

Evidence is deliberately separated:

- **Official capability:** matching upstream export/configuration contracts,
  architectures, model cards, and template implementations.
- **Verified metadata:** all eight profiles were regenerated against their
  immutable public revisions. Seventeen file entries matched byte sizes and
  SHA-256 declarations; the three small token tables were fetched and hashed.
- **Local checks:** profile validation, settings round trips, exact installation
  artifact resolution, menu selection, fixed precision, and synthetic download
  routing. No microphone or new model weights were used for these checks.
- **Unverified:** successful full inference with each new artifact on the user's
  device, cleanup fidelity, WER/CER, latency, RAM, and language-specific quality.
  Existing model measurements must not be attributed to these presets.

The profile expansion described above is frozen-weight catalog/configuration work. It introduces no runtime
decoder/kernel changes, training/checkpoint changes, streaming delivery, or
resident model ownership. The [research evidence rules](research-streaming-inference-papers.md)
continue to govern those separate proposals.

Whisper, SenseVoice, Moonshine, Canary, Qwen3-ASR, and CTC-only exports are not
interchangeable with this profile format. Although the
[upstream runtime defines separate configurations](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/offline-model-config.h)
for more families, the application needs corresponding adapters before listing
them as usable models. Qwen3-ASR and Cohere now have dedicated built-in adapters; they are not imported as transducer profiles. See [their runtime contract](offline-asr.md).

## Maintaining the catalog

Use `scripts/huggingface-profile.mjs` to resolve public artifact metadata and
small-file hashes, then record a precise upstream compatibility source. Keep the
complete profile in `models/presets.json`; do not add a display-only model name.
`modelPresets.test.mjs` checks selection and download identities, while the Rust
`menu_presets_roundtrip_and_resolve_exact_profile_artifacts` test validates the
same catalog through the actual settings and installer contract. The browser
page `/tests/recovery.html` tests explicit save/download and reopening with
synthetic native commands. See [validation instructions](validation.md).

## Automatic audio setup validation

The browser regression page exercises ordered downloads, installed-file reuse,
missing speaker runtime, cancellation/network failure and automatic timed/online recovery,
recording exclusion, unsaved settings, folder changes, and priority for explicit
speech downloads. Native settings tests cover old off/DPDF4/DPDF8 preferences,
read-only migration, save/reset and unchanged core/evaluator defaults.

The opt-in network test downloads the pinned DPDFNet8 and Silero packages into
temporary storage, verifies hashes, checks that reinstalling transfers no files,
repairs a deliberately corrupted file, and runs the managed frontend on a public recording. It passed locally using
FLEURS en_us test `2013850546771709559.wav`, SHA-256
`ca2974ef8698d1aeb132782a02aaf50a18707489d1cb132af7a83efbc72bc236`.
Output is finite, finalized and preserves the resampled sample count; gain stays
disabled. This is a download/integration check, not an ASR accuracy or latency result.
No downloaded weights, recordings or generated reports are versioned.

```sh
. scripts/dev-env.sh
VESPER_SMOKE_AUDIO=/path/to/public.wav cargo test --locked \
  -p vesper-desktop --lib managed_audio_downloads_verify_and_reuse_pinned_packages \
  -- --ignored --nocapture
```
