# Language support and model recommendations

Updated 18 September 2026. This is the canonical product-facing map for transcription languages, model compatibility, benchmark coverage, and future recommendation behavior. Runtime contracts remain authoritative in `vesper-core`; this document explains them without turning an upstream language claim into a local quality claim.

## What “supported language” means

Four different concepts must remain separate:

| Concept | Meaning |
| --- | --- |
| Application language | A recognition mode visible in Dictate and Settings: Automatic, English, French, or Hindi |
| Model compatibility | The selected adapter accepts that application language and backend |
| Benchmark profile | The actual held-out speech population, such as `en-US`, `en-IN`, `hi-IN`, or `hi+en` |
| Qualified recommendation | The exact model artifact passed declared quality, latency, reliability, mode, and device gates on comparable local evidence |

The language selector guides recognition; it does not translate. Indian English and US English both use the English selector but must be evaluated separately. Hinglish currently uses Automatic; it is not a separate picker value. Automatic detection does not add a language that the model does not support.

The current product languages are English, French, and Hindi. Automatic is a detection mode, not a fourth language. German and Spanish appear in one optional FastConformer profile's upstream metadata, but the current application does not expose them as explicit language selections.

## Built-in ASR adapters

Fresh settings select Cohere Transcribe, explicit English, and Qwen3.5 cleanup. Saved user selections remain authoritative.

| Built-in speech model | App language choices | Delivery in this application | Speech backends exposed by the adapter | Current evidence status |
| --- | --- | --- | --- | --- |
| Cohere Transcribe (default) | English or French, explicit only | Completed recording/file; bounded long-audio segments | Automatic or CPU | English completed-path evidence; 48-recording Indian-English screen inside a separate personalization envelope; French integration fixture, not a broad French benchmark |
| NVIDIA Nemotron 3.5 ASR Streaming 0.6B | Automatic, English, French, Hindi | Native live partials and completed audio | CPU, Metal, CUDA, Vulkan; the AMD application profile routes speech through Vulkan | Local CPU Hindi, US-English, and Hinglish screens plus separate Indian-English evidence; Hinglish does not currently pass the quality gate |
| Qwen3-ASR 0.6B | Automatic, English, French, Hindi | Completed recording/file; bounded long-audio segments | Automatic; CPU; Metal for English on Apple Silicon | Local phase-one Hindi, US-English, and Hinglish screening; no qualified Hinglish result |
| NVIDIA Parakeet TDT 0.6B v3 | Automatic, English, French in the picker | Completed recording/file | Automatic; CPU; Metal for English on Apple Silicon | Compatibility/smoke evidence; no current comparable phase-one matrix; Hindi is unsupported |
| Liquid LFM2.5 Audio 1.5B | English only, explicit | Completed recording/file; resident weights and bounded segments | Automatic or CPU | Small English application-path screen only; not broad English or edge-device qualification |

“Automatic” in the backend column means the application may choose an available supported execution path and fall back according to that adapter's policy. It is not evidence that a GPU executed every operator. GPU, driver, precision, and runtime-package combinations require separate measurements.

Only Nemotron currently exposes native live transcription events to the application. Qwen3-ASR, Cohere, Parakeet, and LFM are completed-audio paths here.

## Additional ASR menu profiles

The Models menu also contains three pinned sherpa-onnx transducer profiles, giving eight ASR choices in total. These reuse the custom-profile runtime path rather than adding new built-in architecture adapters.

| Menu profile | Declared profile languages | Languages currently selectable in the app | Delivery/backend | Qualification status |
| --- | --- | --- | --- | --- |
| NVIDIA Parakeet TDT 0.6B v2 INT8 | English | Automatic, English | Completed audio, CPU | 16-recording US-English CPU screen: 5.25% micro-WER and 0.122 median RTF; broader English qualification remains |
| NVIDIA Parakeet Unified English 0.6B INT8 | English | Automatic, English | Completed audio, CPU | 16-recording US-English CPU screen: 5.86% micro-WER and 0.115 median RTF; broader English qualification remains |
| NVIDIA FastConformer European 114M FP32 | English, German, Spanish, French | Automatic, English, French | Completed audio, CPU | 16-recording US-English CPU screen: 9.26% micro-WER and 0.069 median RTF; German/Spanish are not explicit product selections and French remains unmeasured |

The development `vesper-eval --model` command accepts seven built-in ASR enum values, including two Whisper evaluation variants that are withheld from product selection. Its validated `--asr-profile` option evaluates the three pinned menu profiles through the production custom-profile adapter.

Cleanup models are a separate role. Qwen3.5-0.8B and the optional cleanup presets cannot be compared as speech recognizers. Cleanup needs its own fidelity, meaning-preservation, latency, and memory benchmark.

## Whisper evaluation gate

Whisper Small and Turbo now share a real architecture adapter, resident Sherpa ownership and pinned INT8 installation. They run through the application manager in completed-audio English/French mode, including automatic detection restricted to those languages. The picker remains at eight choices: Whisper noise/repetition quality gates have failed, so its recommendation metadata retains `executable: false`. This flag represents release qualification; it does not mean the development adapter is missing. Hindi, code-switching, singing and native Whisper streaming are not qualified. See [the dated adapter regression](validation/whisper-adapter-2026-09-18.md).

## Current numeric benchmark coverage

The Models menu has **eight ASR choices**: five built-in adapters and three custom-profile choices. All eight have results on one comparable 16-recording US-English completed-audio CPU screen. That closes execution coverage only for this narrow profile; it is not an all-language, streaming, or all-device qualification.

The development evaluator accepts one validated `--asr-profile` declaration and routes it through the same production custom-profile adapter. This enabled comparable runs for Parakeet v2, Parakeet Unified English, and FastConformer without a second inference implementation. The [dated result](../benchmarks/asr/2026-09-18-app-catalog-en-us.md) preserves the then-nine-model, 144-execution screen, including Kyutai's failed quality and real-time gates. Kyutai was subsequently removed from application support under [GitHub issue #48](https://github.com/rahulsharmavishwakarma/transcribe/issues/48); historical measurements remain evidence, not a downloadable-model claim.

Coverage remains narrower when filtered by language: Nemotron and Qwen3-ASR have Hindi and Hinglish screening; Nemotron and Cohere have a separate Indian-English screen inside a personalization experiment envelope, which is deliberately excluded from base-model recommendation ranking. Qwen3.5 cleanup has end-to-end and fidelity measurements, but it is a text rewriter and is not part of the eight-ASR count. Therefore the benchmark now covers every current app-catalog ASR choice only for the specific US-English CPU lane and still cannot produce a complete language/device recommendation.

## Language-by-language view

| User need | Candidate models before benchmarking | Required benchmark lane | Current recommendation state |
| --- | --- | --- | --- |
| US/general English | All five built-ins plus the three ASR profiles, subject to their picker restrictions | `en-US`; explicit English and, where supported, Automatic kept separate | All eight current choices have one comparable explicit-English CPU screen; the small reused pack does not establish a broad or cross-device winner |
| Indian English | Same English-compatible candidates | `en-IN`, kept separate from `en-US` | A 48-recording Svarah CPU screen measured Nemotron and Cohere; it belongs to a personalization experiment envelope and is excluded from automatic base-model recommendation aggregation |
| Hindi | Nemotron and Qwen3-ASR | `hi-IN`; explicit Hindi and Automatic separate | Both have local CPU screening; broader device/noise coverage remains |
| Hinglish/code switching | Qwen3-ASR advertises code switching; Nemotron is retained as an Automatic Hindi/English research comparator but its capability flag does not claim code switching | `hi+en`, mixed-script scoring plus qualitative review | No qualified model under the current phase-one quality ceiling |
| French | Nemotron, Qwen3-ASR, Cohere, Parakeet v3, and FastConformer | `fr-FR` or another declared locale; explicit and Automatic separate | Fixture/runtime coverage exists, but no comparable all-model French recommendation |
| German or Spanish | FastConformer metadata describes these languages; Parakeet has broader upstream European coverage | A new explicit product-language contract and held-out locale pack | Not currently an explicit application language; do not recommend through the picker |
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
