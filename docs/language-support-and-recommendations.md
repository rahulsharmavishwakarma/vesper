# Language support and model recommendations

Updated 27 September 2026. This is the canonical product-facing map for transcription languages, model compatibility, benchmark coverage, and future recommendation behavior. Runtime contracts remain authoritative in `vesper-core`; this document explains them without turning an upstream language claim into a local quality claim.

## What “supported language” means

Four different concepts must remain separate:

| Concept | Meaning |
| --- | --- |
| Application language | A recognition mode visible in Dictate and Settings: Automatic plus the selected model's vendor-declared languages (`models/languages.json`) |
| Model compatibility | The selected adapter accepts that application language and backend |
| Benchmark profile | The actual held-out speech population, such as `en-US`, `en-IN`, `hi-IN`, or `hi+en` |
| Qualified recommendation | The exact model artifact passed declared quality, latency, reliability, mode, and device gates on comparable local evidence |

The language selector guides recognition; it does not translate. Indian English and US English both use the English selector but must be evaluated separately. Hinglish currently uses Automatic; it is not a separate picker value. Automatic detection does not add a language that the model does not support.

The product languages follow the selected speech model: each menu shows that model's vendor-declared languages from `models/languages.json` (up to 30 for Qwen3-ASR). Automatic is a detection mode, not a language, and only models whose vendor declares auto-detect offer it. English, French, and Hindi were the previous fixed subset. German and Spanish are explicit selections for the models that declare them and remain resolvable for custom profiles declaring those tags.

## Built-in ASR adapters

Fresh settings select Cohere Transcribe, explicit English, and Qwen3.5 cleanup. Saved user selections remain authoritative.

| Built-in speech model | App language choices | Delivery in this application | Speech backends exposed by the adapter | Current evidence status |
| --- | --- | --- | --- | --- |
| Cohere Transcribe (default) | 14 vendor-declared languages, explicit only, no auto-detect, no Hindi | Buffered live drafts; completed recording/file final | Automatic or CPU | English completed-path evidence; 48-recording Indian-English screen inside a separate personalization envelope; French integration fixture, not a broad French benchmark |
| NVIDIA Nemotron 3.5 ASR Streaming 0.6B | Automatic plus 28 out-of-the-box locales (8 adaptation locales excluded) | Native live partials and completed audio | CPU, Metal, CUDA, Vulkan; the AMD application profile routes speech through Vulkan | Local CPU Hindi, US-English, and Hinglish screens plus separate Indian-English evidence; Hinglish does not currently pass the quality gate |
| Qwen3-ASR 0.6B | Automatic plus 30 declared languages | Native MLX or buffered CPU drafts; completed recording/file final | Automatic; CPU; Metal for English on Apple Silicon | Local phase-one Hindi, US-English, and Hinglish screening; no qualified Hinglish result |
| NVIDIA Parakeet TDT 0.6B v3 | Automatic plus 25 declared European languages | Buffered live drafts; completed recording/file final | Automatic; CPU; Metal for English on Apple Silicon | Compatibility/smoke evidence; no current comparable phase-one matrix; Hindi is unsupported |
| Liquid LFM2.5 Audio 1.5B | English only, explicit | Buffered live drafts; resident weights and completed-audio final | Automatic; CPU; Metal; CUDA/Vulkan/ROCm with matching builds | English CPU/Metal evidence; non-Apple GPU qualification deferred to #76 |
| OpenAI Whisper Small | Automatic, English, French (qualified claim, not vendor-researched) | Completed recording/file final, INT8 CPU | Automatic or CPU | Adapter validation record 2026-09-18; offered by owner decision 2026-10-07 despite failed noise/repetition gates |
| OpenAI Whisper Turbo | Automatic, English, French (qualified claim, not vendor-researched) | Completed recording/file final, larger encoder, INT8 CPU | Automatic or CPU | Same adapter record and picker status as Whisper Small |

“Automatic” in the backend column means the application may choose an available supported execution path and fall back according to that adapter's policy. It is not evidence that a GPU executed every operator. GPU, driver, precision, and runtime-package combinations require separate measurements.

All current ASR adapters expose live drafts. Nemotron uses native cache-aware streaming; English/Metal Qwen uses native MLX previews. Other paths use bounded buffered previews. All except Nemotron retain the completed-audio final pass. See [live adapters](live-transcription.md) for measured scope, finalization and resource limits; historical completed-audio quality results do not qualify preview quality.

## Language data source and code generation

`models/languages.json` is the single human-edited source of record for per-model language support (Refs #100). Each speech entry holds `languages` (explicit ISO codes), `autoDetect`, `source`, and optional `runtimeCodes`: the per-runtime values inference backends require (NeMo locales, Qwen display names, sherpa ISO codes). Models that need no runtime value declare none (Parakeet auto-detects; LFM is English-only). The `writers` section declares built-in writer support the same way; `unrestricted: true` marks writers whose vendor publishes no per-language list, in which case every application mode stays allowed and nothing is invented.

`vesper-core/build.rs` generates the adapter language tables, the runtime-code lookup, and the writer table from the manifest at compile time; a malformed manifest fails the build naming the offending key. The agreement test pins each model's adapter capabilities to its manifest set, and per-language tests on both the speech and writing paths read the manifest directly, so extending it extends coverage with no test edit. Menus (frontend) read the manifest file; validation (backend) reads the generated tables, so the two cannot drift.

To onboard a model: add one manifest block with vendor-declared languages researched from the vendor page at a recorded revision (never guessed), wire one ADAPTERS registration plus its runtime adapter code, and every menu, validation, and per-language test follows with no further list edits. Product selection stays a separate qualification decision via the catalog `status`: withheld entries never reach the picker. The `Language` enum itself is unchanged: adding a *language* still touches the enum, its ISO code, and the frontend options.

Picker display (names, creators, blurbs) comes from `models/catalog.json`, not code: the frontend `modelOptions` and the backend `default_model_catalog()` both read the catalog, with language counts from `models/languages.json`. Withheld catalog statuses stay out of the picker by construction, and parity tests pin both directions. Only the selectable-id contract (`ASR_MODELS` keys, `CLEANUP_MODELS` keys) stays code-side with the backend enums, which reject unknown ids.

## Additional ASR menu profiles

The Models menu also contains three pinned sherpa-onnx transducer profiles, giving ten ASR choices in total. These reuse the custom-profile runtime path rather than adding new built-in architecture adapters.

| Menu profile | Declared profile languages | Languages currently selectable in the app | Delivery/backend | Qualification status |
| --- | --- | --- | --- | --- |
| NVIDIA Parakeet TDT 0.6B v2 INT8 | English | Automatic, English | Buffered live drafts and completed audio, CPU | 16-recording US-English CPU screen: 5.25% micro-WER and 0.122 median RTF; broader English qualification remains |
| NVIDIA Parakeet Unified English 0.6B INT8 | English | Automatic, English | Buffered live drafts and completed audio, CPU | 16-recording US-English CPU screen: 5.86% micro-WER and 0.115 median RTF; broader English qualification remains |
| NVIDIA FastConformer European 114M FP32 | English, German, Spanish, French | Automatic, English, German, Spanish, French | Buffered live drafts and completed audio, CPU | 16-recording US-English CPU screen: 9.26% micro-WER and 0.069 median RTF; German/Spanish now resolve as explicit selections and French remains unmeasured |

The development `vesper-eval --model` command accepts seven built-in ASR enum values, including two Whisper variants now also offered in product selection by owner decision 2026-10-07. Its validated `--asr-profile` option evaluates the three pinned menu profiles through the production custom-profile adapter.

Cleanup models are a separate role. Qwen3.5-0.8B and the optional cleanup presets cannot be compared as speech recognizers. Cleanup needs its own fidelity, meaning-preservation, latency, and memory benchmark.

## Writing language compatibility

Dictate and Settings use the same compatibility policy. Their language menu
starts with the selected ASR's current application choices and selected backend.
Explicit Metal on Parakeet or Qwen ASR exposes English only; Automatic retains
the other compatible choices because the runtime can choose CPU. With polishing
enabled, the menu intersects those choices with the writer's declared languages.
Changing a model never silently changes the saved language. An inline error
blocks saving an incompatible combination; disabling polishing restores the
speech model's choices.

| Writing selection | Explicit product languages | Evidence and boundary |
| --- | --- | --- |
| Built-in Qwen3.5 0.8B | All application modes (no per-language list published) | Vendor claims 201 languages and dialects (model card, 2026-10-07); third-party note: quality is not guaranteed for low-resource languages |
| Qwen2.5 0.5B / 1.5B Q4_K_M presets | English, French | Both are named by the pinned model cards; Hindi is conservatively withheld without exact checkpoint evidence/qualification |
| SmolLM2 360M Q8_0 / 1.7B Q4_K_M presets | English | Official cards describe primarily English behavior |
| Gemma 3 1B Q4_K_M preset | English | Exact-size release guidance distinguishes the 1B model from larger multilingual variants; family-wide claims are insufficient |
| Other saved profiles | Declared language tags within the shared language options | Base tags match case-insensitively, e.g. `fr-FR` maps to French; undeclared support is not inferred |

Custom writers require an explicit language. Automatic detection is not a
language declaration, and the current completion contract has no reliable
detected-language field with which to establish custom-writer compatibility.
This conservative restriction does not claim that a custom model cannot process
other languages. It also does not establish code-switch fidelity for the built-in
writer. The existing writing instructions preserve the source language; they
cannot add multilingual capability to a checkpoint.

Legacy profiles with empty language metadata still load without rewriting the
preferences file. Reselect the preset in Models, select the built-in writer, or
disable polishing to repair the combination. Native settings saves reject an
incompatible enabled writer. The shared rewriting entry point also checks before
queuing work, so an unrepaired legacy configuration preserves the raw transcript
through the existing warning/fallback path. Application memory remains available
after recognition, independently of these model compatibility checks.

Official evidence reviewed 27 September 2026:

- [Qwen3.5 pinned source card](https://huggingface.co/Qwen/Qwen3.5-0.8B/blob/2fc06364715b967f1860aea9cf38778875588b17/README.md) and [official multilingual release](https://qwen.ai/blog?id=qwen3.5). The application's GGUF revision remains `8fea620810c4afa23dd6443f999a48574c1611a3`.
- [Qwen2.5 0.5B pinned GGUF card](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/blob/9217f5db79a29953eb74d5343926648285ec7e67/README.md) and [1.5B pinned card](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/blob/91cad51170dc346986eccefdc2dd33a9da36ead9/README.md).
- [SmolLM2 360M limitations](https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct#limitations), [360M pinned GGUF card](https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct-GGUF/blob/593b5a2e04c8f3e4ee880263f93e0bd2901ad47f/README.md), and [1.7B pinned card](https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF/blob/2d4a76a30b4af41ecd395c35725ac11688d4cfe4/README.md).
- [Gemma 3 integration release, exact-size table](https://huggingface.co/blog/gemma3) versus the family-wide wording in the [pinned 1B GGUF card](https://huggingface.co/ggml-org/gemma-3-1b-it-GGUF/blob/f9c28bcd85737ffc5aef028638d3341d49869c27/README.md). English is a conservative product boundary, not proof of inability to produce other languages.

This is a frozen-weight application configuration change, not a decoder/kernel
or training technique. Runtime pins remain sherpa-onnx
`11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf` with the existing completion patch,
MLX-Audio-Swift `d302a5c6080d2bb97bae38c7418f82abb76013b6`, and llama.cpp
`d81235049384534c167caea52b85a694f6103d14` (it was `e79e4bf6` until the 0.1.159 upgrade, so sessions saved
earlier record that revision); model hashes are unchanged.
Contract tests cover language menus, backend boundaries, legacy recovery and
pre-load rejection. These are local software checks, not multilingual inference
measurements. Official coverage, paper results, local quality measurements and
untested hypotheses must remain separate; no quality ranking or default change
follows from this metadata update.

## Whisper evaluation gate

Whisper Small and Turbo now share a real architecture adapter, resident Sherpa ownership and pinned INT8 installation. They run through the application manager in completed-audio English/French mode, including automatic detection restricted to those languages. Whisper noise/repetition quality gates have failed, and the catalog carried them as withheld; the owner overrode this on 2026-10-07 to offer both variants in the picker. The failures stand as recorded evidence and still exclude Whisper from qualified recommendation. Hindi, code-switching, singing and native Whisper streaming are not qualified. See [the dated adapter regression](validation/whisper-adapter-2026-09-18.md).

## Current numeric benchmark coverage

The Models menu has **ten ASR choices**: seven built-in adapters and three custom-profile choices. Eight of the ten have results on one comparable 16-recording US-English completed-audio CPU screen; the Whisper pair has adapter validation only (2026-09-18), not that screen. That closes execution coverage only for this narrow profile; it is not an all-language, streaming, or all-device qualification.

The development evaluator accepts one validated `--asr-profile` declaration and routes it through the same production custom-profile adapter. This enabled comparable runs for Parakeet v2, Parakeet Unified English, and FastConformer without a second inference implementation. The [dated result](../benchmarks/asr/2026-09-18-app-catalog-en-us.md) preserves the then-nine-model, 144-execution screen, including Kyutai's failed quality and real-time gates. Kyutai was subsequently removed from application support under [GitHub issue #48](https://github.com/rahulsharmavishwakarma/vesper/issues/48); historical measurements remain evidence, not a downloadable-model claim.

Coverage remains narrower when filtered by language: Nemotron and Qwen3-ASR have Hindi and Hinglish screening; Nemotron and Cohere have a separate Indian-English screen inside a personalization experiment envelope, which is deliberately excluded from base-model recommendation ranking. Qwen3.5 cleanup has end-to-end and fidelity measurements, but it is a text rewriter and is not part of the ten-ASR count. Therefore the benchmark now covers every current app-catalog ASR choice only for the specific US-English CPU lane and still cannot produce a complete language/device recommendation.

## Language-by-language view

| User need | Candidate models before benchmarking | Required benchmark lane | Current recommendation state |
| --- | --- | --- | --- |
| US/general English | All five built-ins plus the three ASR profiles, subject to their picker restrictions | `en-US`; explicit English and, where supported, Automatic kept separate | All eight current choices have one comparable explicit-English CPU screen; the small reused pack does not establish a broad or cross-device winner |
| Indian English | Same English-compatible candidates | `en-IN`, kept separate from `en-US` | A 48-recording Svarah CPU screen measured Nemotron and Cohere; it belongs to a personalization experiment envelope and is excluded from automatic base-model recommendation aggregation |
| Hindi | Nemotron and Qwen3-ASR | `hi-IN`; explicit Hindi and Automatic separate | Both have local CPU screening; broader device/noise coverage remains |
| Hinglish/code switching | Qwen3-ASR advertises code switching; Nemotron is retained as an Automatic Hindi/English research comparator but its capability flag does not claim code switching | `hi+en`, mixed-script scoring plus qualitative review | No qualified model under the current phase-one quality ceiling |
| French | Nemotron, Qwen3-ASR, Cohere, Parakeet v3, and FastConformer | `fr-FR` or another declared locale; explicit and Automatic separate | Fixture/runtime coverage exists, but no comparable all-model French recommendation |
| German or Spanish | FastConformer metadata describes these languages; Parakeet, Nemotron, Qwen3-ASR, and Cohere declare them | A held-out locale pack | Explicit application languages for the declaring models; do not recommend through the picker without a comparable run |
| Singing/vocals | No model is presently product-qualified | Separate singing corpus, lyrics policy, vocal conditions, and latency/quality gates | Unsupported as a recommendation claim |

Mixed-script WER treats a Latin word and its Devanagari transliteration as different. Hinglish decisions therefore require exact WER/CER plus qualitative review of meaning, entities, numbers, script choice, and code-switch boundaries.

## Benchmark and recommendation contract

The benchmark is development tooling and is not shipped with the application. Corpora and ordinary raw machine reports remain outside the application bundle. A benchmark run uses the real decoder and inference adapter, then records model/runtime provenance, artifact hashes, language/configured-language, mode, actual backend envelope, WER/CER, timing, failures, and streaming measurements where applicable.

Recommendation filtering happens before ranking:

1. Require an executable application adapter and compatible language, mode, backend, duration, Automatic behavior, code-switching need, and singing need.
2. Compare only identical corpus, configured language, mode, backend, precision, runtime, and hardware envelopes.
3. Preserve failed samples in the denominator.
4. Apply any requested maximum median WER, median real-time factor, p95 stop-to-final latency, and failure rate.
5. Rank qualifying local measurements ahead of official capability-only candidates using the requested accuracy, latency, or balanced objective.
6. Return no qualified model when every comparable candidate fails. Never relabel a failed local result as unmeasured official support.

The checked-in request examples currently use a 20% median-WER ceiling for monolingual screening, 30% for Hinglish, 1.0 median RTF, and 2% failure rate. These are initial screening policy values, not universal scientific thresholds. Production thresholds need a representative validation set and review of names, numbers, negation, meaning preservation, accents, noise, silence, long recordings, memory, power, and thermals.

## What the application recommends today

The application currently filters the language menu and download availability using compatibility metadata. It does **not** yet consume benchmark evidence to show “best model for you,” automatically switch models, or automatically download a recommendation. The evaluator can generate evidence-backed recommendation JSON for development review.

A future user-facing recommendation should display four distinct states:

| State | User-facing meaning |
| --- | --- |
| Recommended for this measured profile | Passed comparable local gates for the requested language/mode/device envelope |
| Compatible, not yet measured here | Official/runtime compatibility exists, but there is no comparable local result |
| Did not pass this profile | A comparable run exceeded a declared quality, latency, or reliability gate |
| Unavailable | Incompatible language/mode/backend, missing runtime, or non-executable architecture |

Installation must remain explicit. A recommendation may explain the expected download size, runtime requirement, backend, evidence date, and why it was selected, but it must not download or change the active model without the user's action. Saved settings must remain authoritative, and a newer model release is only a candidate until its pinned artifact and architecture-specific adapter pass the same held-out manifests.

## Coverage still required for a useful user recommendation

- Run a clean base-model Indian-English matrix across valid models; retain the existing 48-recording Svarah personalization screen as separate evidence.
- Add a held-out French pack and comparable explicit/Automatic runs.
- Run all five built-ins and three profiles on every compatible CPU lane.
- Qualify Metal, CUDA, Vulkan, and the AMD application profile on named hardware without pooling them with CPU.
- Add representative noise, silence, accents, long-form, cancellation/reset, memory, power, and sustained-thermal cases.
- Keep live Nemotron measurements separate from completed-audio models and report first partial, endpoint/stop-to-final, partial stability, queue drops, and final accepted-text latency.
- Evaluate cleanup independently and end to end; a fast ASR result does not prove the cleaned transcript is faithful.

See the [ASR benchmark contract](../benchmarks/asr/README.md), [latest sanitized application-path screen](../benchmarks/asr/2026-09-15-screen.md), [inference performance](inference-performance.md), [model presets](model-presets.md), and [ADR-0010](adrs/0010-evidence-gated-asr-recommendations.md).

Automatic speech downloads now choose the qualified English MLX export for Parakeet/Qwen on compatible Apple Silicon builds; other languages and automatic language detection retain their supported CPU export. Existing CPU installations remain usable while an optimized update is available. This is a runtime/artifact preference, not a language or default-model change. See [automatic acceleration](accelerators.md#building-the-apple-silicon-application) and the dated Automatic qualification in [inference performance](inference-performance.md).
