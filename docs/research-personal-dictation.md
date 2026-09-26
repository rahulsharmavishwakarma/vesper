# Personal dictation that improves with use

Research reviewed: 16 September 2026. Recent-evidence window: **16 September 2024–16 September 2026**, using the first public paper date. Older work is explicitly separated as background.

The recommended product promise is **fewer recurring corrections for this user, with meaning preserved and every remembered preference under their control**. This is a product hypothesis to validate, not a measured capability claim.

The follow-up [automatic-personalization research](research-automatic-personalization.md) examines learning from ordinary edits, vocabulary usage and repeat dictation without per-entry confirmation. It documents recent primary evidence, the missing interaction signals and a chronological evaluation plan. Phase A of that extension is now implemented under [ADR-0016](adrs/0016-automatic-local-spelling-learning.md); see the detailed [implementation guide](personalization/README.md). The table below also identifies the preserved explicit-review capabilities.

This report extends the existing [research](research.md), [reviewed-personalization decision](adrs/0012-adapter-registry-and-reviewed-personalization.md), and [continuous improvement contract](self-improvement.md). The user approved the on-device, model-independent implementation in [ADR-0015](adrs/0015-model-independent-local-memory.md); training is deferred until a separately approved own-model project. Approval decisions, completion criteria and progress belong to [personalization issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34) and [training issue #5](https://github.com/rahulsharmavishwakarma/transcribe/issues/5).

## What the application actually does

The application owns memory in its existing local SQLite profile, independently of ASR and cleanup model identity. No cloud inference, embedding API, hosted memory service, or training job is introduced. Model downloads remain an explicit existing setup operation; dictation and memory use installed local components.

The worktree contains substantial uncommitted source beyond Git HEAD `592e0084fe5cf486b4d56c575423e9a4f219bfb8`; HEAD alone does not reproduce this implementation. Synthetic tests establish behavior, not personal accuracy improvements.

| Capability | Implemented now | Boundary |
| --- | --- | --- |
| 1. Correction learning | Automatic spelling learning from eligible ordinary edits in Your text, plus the existing explicit review/remember path. | The September 18 product amendment removes the learning opt-in for new/existing desktop profiles. Admission stays narrow; it does not observe external-editor edits or treat machine repetition/copying as verified spelling. |
| 2. Contextual vocabulary | Up to 128 entries, eight aliases each, language/project/context scope, pause/remove controls. Apply exact confirmed aliases in a separate final-text stage. | Works with cleanup off and any text-producing ASR adapter. Phrase boosting remains adapter-specific; no fuzzy/phonetic decoder is added. |
| 3. Evidence of improvement | Counts actual uses/undos separately from bounded paired text replay against later verbatim reviews; keeps improved and worsened cases. | Excludes source/duplicate audio and memories saved after the evaluated recording. This is not an acoustic benchmark or proof of end-to-end improvement. |
| 4. Useful feedback | At most four observed-error cases with language/day diversity plus one otherwise-unflagged audit case. | Deterministic review aid, not calibrated ASR confidence or a representative random sample. |
| 5. Supplied intent | Explicit output forms plus a suggestion when at least two distinct remembered sessions agree among the top three relevant examples; automatic use is opt-in. | Lexical agreement, not a trained intent classifier. No inferred recipient, commitment, action execution or automatic project detection. |
| 6. Personal pronunciation confusions | Explicitly mark a recurring mishearing as a pronunciation-related alias; retain reviewed sources and context. | This adapts text correction to observed errors. It does not detect accents or change acoustic weights. |

Sources: [profile contract](../crates/vesper-core/src/personalization.rs), [deterministic memory](../crates/vesper-core/src/memory.rs), [cleanup retrieval](../crates/vesper-inference/src/personalization.rs), [review and replay validation](../crates/vesper-data/src/personalization.rs), and [adapter capabilities](../crates/vesper-core/src/adapters.rs).

**Memory portability does not mean identical benefit with every model.** A new recognizer may make different errors. The same profile survives, while the learning report separates model/language and stored provenance/settings groups. Raw ASR, model cleanup, personalized output and human reviews remain separate. Undo restores the machine fallback; ambiguous overlapping aliases leave the wording unchanged. The [16 September native corpus screen](../benchmarks/asr/2026-09-16-personalization.md) now measures one frozen profile across two recognizers: small Hindi orthographic gains, an English initialism scoring regression, and no Indian English memory applications. It does not establish longitudinal personal benefit.

**The Cohere adapter has no recognition phrase hints.** The separate local correction stage still operates on its output. Optional Nemotron recognition hints use only canonical spellings, selected from context available before recording, with the existing 32-phrase/2048-byte limit and uniform boost 2. Cleanup retains its own fidelity guard; memory does not grant it permission to rewrite freely.

The [expanded regression screen](../benchmarks/asr/2026-09-16-memory-regressions.md) subsequently fixed quoting/negation/suggestion boundaries and dataset parsing, passed 778 controlled cases, and measured the same remembered English name correction with both recognizers on fresh recordings. These are bounded application-memory results; real personal learning and intent/style benefit remain unqualified.

## What is already common in competing products

These are official product descriptions checked on the research date, not independently measured comparisons or evidence of competitors' internal training methods.

| Product | Documented personalization | Consequence for differentiation |
| --- | --- | --- |
| Wispr Flow | Vocabulary and misspelling replacements; vendor says corrected spellings can enter its dictionary; category-specific styles and explicit speech backtracking. | A dictionary, replacement rules and style modes alone are insufficient differentiation. [Dictionary](https://docs.wisprflow.ai/articles/4052411709-teach-flow-your-words-with-the-dictionary), [features](https://try.wisprflow.ai/), [styles](https://docs.wisprflow.ai/articles/2368263928-how-to-setup-flow-styles), [backtracking](https://docs.wisprflow.ai/articles/5373093536-how-do-i-use-smart-formatting-and-backtrack). |
| Superwhisper | Vocabulary hints and post-transcription replacements. Its July 2026 CLI/MCP workflow mines local history for vocabulary and tone patterns with approval. | Local memory mining is also emerging elsewhere. An integrated, low-effort learning experience and evidence of benefit matter. Local history does not imply that an externally chosen analysis model runs locally. [Vocabulary](https://superwhisper.com/docs/get-started/interface-vocabulary), [CLI/MCP](https://superwhisper.com/blog/cli-mcp). |
| Aqua Voice | Custom dictionary, standing style instructions and synchronized preferences; its FAQ explicitly says no per-user voice training. | Distinguish preference/context adaptation from acoustic weight adaptation. Its FAQ describes cloud inference. [Official FAQ](https://aquavoice.com/info/faq). |

The proposed advantage combines user-specific accuracy, a small correction burden, faithful intent handling, local processing, and inspectable/reversible memory. This survey does not establish that no competitor offers any individual element.

## Three improvement loops

| Loop | What changes | When value could appear | Honest description |
| --- | --- | --- | --- |
| Correction memory | Approved vocabulary, confusion aliases and presentation preferences | A later relevant dictation | The application remembers a correction; model weights stay unchanged. |
| Context selection | Which approved memories apply for the selected language, project and output form | Each recording | The application chooses relevant evidence; it does not know unspoken facts. |
| Acoustic/model adaptation | A separately trained and qualified candidate checkpoint | After enough reviewed evidence and evaluation | The recognizer or cleanup model is adapted; gains must survive native export and later-session tests. |

The first two loops are the approved application scope, now including the optional automatic spelling path in ADR-0016. Training is deferred until the user publishes their own model and approves a concrete training proposal. A user should not need to understand these technical distinctions in the recording flow, but product claims and the memory inspector must remain truthful.

## Design rationale and qualification work

### 1. Turn corrections into small, reviewable suggestions

After an explicit review, suggest the changed name or technical term with its original confusion, language and context. For a synthetic example, an approved correction from “cube control” to `kubectl` can teach a technical spelling. One action should confirm the proposed memory; repeated full-transcript labeling should not be required for every preference.

Keep supervision types separate: audio/verbatim correction, spelling preference, presentation edit, resolved spoken self-correction, and newly added content. A user-added recipient is not proof that the recipient was spoken. Existing thumbs-up/down, copying, leaving text unchanged and model output are not ground truth.

Offer explicit actions to remember a spelling, remember a presentation preference, or correct only this instance. Aggregate repeated evidence into one entry and retain its supporting review IDs; contradictory corrections should produce a review request, not two competing global replacements.

### 2. Remember confusions in context

Store approved aliases as retrieval keys for the canonical spelling. Do not boost the erroneous alias as a desired ASR output. Begin with manually selected language/project scope and bounded lexical/alias matching; phonetic lookup is a later experiment if ordinary aliases miss enough cases.

An alias such as “cube control” is useful in Kubernetes dictation but must not rewrite an unrelated sentence about a cube. Prefer a suggestion when context or the intended span is ambiguous. Preserve numbers, negation, quoting, identifiers and untouched spans.

Pre-ASR selection can use only context already available at recording start, such as an explicitly selected project. Post-ASR lookup can use the raw transcript. Do not infer a missing pre-ASR context from text that does not exist yet. Automatic reading of other applications is a separate proposal under issue #4.

Show the relevant memory, its source review and its scope when the user inspects a change. “Applied” is observable; “helped” requires a reviewed outcome or comparison. Support undo, pause and removal, with new recordings using the updated profile.

### 3. Make improvement measurable

Measure repeated errors on later recordings and the actual effort needed to reach acceptable text. Pair quality tests with a frozen profile on identical audio; a before/after weekly graph alone can be confounded by easier topics, microphone changes or a different model.

Useful product measures include corrections per 100 dictated words, active editing seconds per 100 words, recurring entity-error rate, false memory application, explicit acceptance, and undo frequency. Report reviewed coverage and denominators. Saving more words in a dictionary is not an accuracy improvement.

### 4. Capture supplied intent faithfully

Separate output form from speech repair and from actions. “Send it Friday, sorry Monday” contains a supplied repair; “maybe next week” does not contain a firm date. “I don't think we should ship” must retain negation and uncertainty. “I actually enjoyed it” is not a self-correction simply because it contains “actually.”

Keep explicit modes first. The implemented optional intent suggestion uses agreement and an ambiguous fallback; its usefulness still needs a separately reviewed label set. Personal memory can help spell a mentioned project; it must not invent an unspoken recipient, amount or commitment. Drafting, summarization and external action execution need their own explicit product contracts.

### 5. Ask for the most useful feedback

Offer a small optional review queue combining recurring mistakes and diverse language, term and recording conditions. Include an audit of apparently successful dictations: reviewing only flagged errors cannot establish overall accuracy. The implemented audit chooses an otherwise-unflagged recent recording deterministically; a randomized study remains necessary for unbiased quality estimation.

Use uncertainty only when the exact adapter exposes an appropriate signal and it is calibrated on reviewed data. The pinned NeMo ABI describes one alternative; N-best rescoring is not currently available through that interface. A second model's disagreement can be studied offline, but it adds compute and is not an automatic correctness label.

A confidence display should not manufacture numeric certainty from text length or an LLM's self-reported score. Keep the initial selection heuristic explainable and measure whether it finds more useful corrections per minute than random sampling.

### 6. Adapt to the person before adding an accent label

Accent identification helps only if a downstream model or routing policy uses it and improves outcomes. It is distinct from language identification, microphone adaptation and code-switch recognition. Begin with the user's observed error patterns and optional short calibration recordings across real environments.

A promising initial focus is technical dictation in Indian English and Hinglish, with exact names and identifiers. This is a recommended product focus, not an inferred fact about the user or a supported-language promise. The [existing benchmark](../benchmarks/asr/README.md) records unqualified Hinglish results; the [new Indian English subset](../benchmarks/asr/2026-09-16-personalization.md) provides a bounded CPU measurement, with no observed memory gain. Retain explicit language/mode/backend compatibility filters and the existing default until evidence supports a change.

Later, compare a frozen recognizer with architecture-preserving attention/bias adaptation and full fine-tuning plus replay. Data budgets such as 5, 15 and 30 minutes are experimental conditions, not promises of sufficient data. Preserve separately reviewed verbatim targets, language conditioning, tokenizer and supported streaming contexts. Evaluate new sessions, microphone changes, general speech and other supported languages to detect forgetting.

## Primary research and engineering implications

Categories below follow the [canonical paper-to-engineering map](research-streaming-inference-papers.md): **1** frozen-weight application/public-API work; **2** native decoder/kernel/internal-state work; **3** training/checkpoint changes. Category 1 does not waive architecture approval for an expanded subsystem or data flow.

| Recent primary source | Paper evidence | Application consequence and boundary |
| --- | --- | --- |
| [GEC-RAG, 18 January 2025](https://arxiv.org/html/2501.10734v1) | Stores ASR predictions with ground truths, retrieves lexical examples, and corrects a black-box recognizer without task fine-tuning. Experiments use Whisper-large-v3 and GPT-4o on Persian. | Supports separating correction knowledge from ASR weights. Our deterministic aliases and local cleanup reuse the principle, not its cloud backend, complete algorithm, or reported gains. No verified executable author implementation was found. Category 1. |
| [Evaluating ASR Confidence Scores, 19 March 2025](https://arxiv.org/abs/2503.15124) | User-assisted correction study with 36 participants and nine recognizers; confidence highlighting alone did not improve correction efficiency/usefulness. | Use observed corrections, rejection and diversity for review selection. Do not invent confidence percentages. The English/German study does not establish our queue's benefit. Category 1 hypothesis. |
| [Ontology Memory-Augmented ASR Correction, 11 June 2026; v2 12 June](https://arxiv.org/html/2606.13464v2) | Training-free entity/variant/confusion memory improves over direct LLM correction in nine of ten evaluated settings. Several settings still underperform raw ASR. Local vLLM/H100 Chinese experiments. | Supports structured confusion memory and a raw fallback, while requiring false-substitution tests. Our lightweight exact matcher is not its ontology system. The linked [author repository](https://github.com/fangfang123gh/ontology-asr-correction) contained only a short README when inspected, so it is not a reproducible runtime dependency. Category 1 concept. |
| [MemConflict, 20 May 2026](https://arxiv.org/html/2605.20926v1) | Distinguishes contradictory, context-dependent and changing memories; retrieving memory does not ensure correct use. [Official benchmark](https://github.com/TaoZhen1110/MemConflict). | Scope corrections and abstain when overlapping entries disagree. Do not apply “latest wins” globally. Its external-model benchmark is not integrated. Category 1 engineering hypothesis. |
| [A-MEM, 17 February 2025; NeurIPS 2025](https://arxiv.org/html/2502.12110v11) | Structured external memories with generated metadata, links and evolving relationships. [Official system](https://github.com/WujiangXu/A-mem-sys) includes local backend options. | Memory can be managed outside weights, but our bounded deterministic SQLite profile does not implement A-MEM. Adding its LLM metadata/embedding stack is unnecessary for the approved scope. |
| [Mem0, 28 April 2025](https://arxiv.org/html/2504.19413v1) | Extracts facts and decides add/update/delete/no-op; reported evaluation uses GPT-4o-mini. | The explicit memory lifecycle is relevant. This app uses validated human confirmation, not that backend or a new memory framework. Category 1 concept. |
| [Mind the Pause, May 2026 / ACL 2026](https://aclanthology.org/2026.acl-long.2137/) | Trained disfluency processing for Hindi, Bengali and Marathi using MuRIL and 3B language models. [Author implementation](https://github.com/deepak-kumar-98/Mind-the-Pause). | Category 3, deferred. Use its repair taxonomy to design tests; it does not establish faithful repair handling for current Qwen or Hinglish. |
| [TurboBias 2.0, August 2026](https://arxiv.org/html/2608.21343v1) | Contextual boosting graphs and streaming transducer decoding with official NeMo implementation references. | Category 2 support would be required in the selected native runtime. Current public phrase hints are not TurboBias 2.0. |
| [Adapt4Me demonstration, 20 March 2026](https://arxiv.org/html/2603.20112v1) | Guided correction and VI-LoRA adaptation for non-normative speech; cloud training/backend, including an RTX 3090 setup. | Review-flow inspiration only. Category 3 acoustic training is deferred; this is not evidence for fully on-device continuous training in our app. |
| [Efficient Rehearsal for Continual ASR, January 2026](https://arxiv.org/abs/2601.18266) | Fine-tuning, SVD-gated updates and replay address forgetting in evaluated tasks. [Author implementation](https://github.com/StevenVdEeckt/efficient-rehearsal-for-cl-in-asr). | Category 3, deferred. Its ESPnet/OWSM results do not establish native Nemotron compatibility. |
| [Lost in Transcription, Findings ACL 2025](https://aclanthology.org/2025.findings-acl.1190/) | WER can conceal hallucination behavior. | Evaluate added facts and meaning changes separately from word edits. Fluency is not evidence of fidelity. |

The earlier report included older work; it was **not exclusively a two-year survey**. Historical background only: [phonetic retrieval, first public 11 September 2024](https://arxiv.org/html/2409.15353v1), which falls five days outside this window despite its ICASSP 2025 venue; [Disfl-QA, 2021](https://aclanthology.org/2021.findings-acl.293/); [rapid Conformer speaker adaptation, 2021](https://www.isca-archive.org/interspeech_2021/huang21c_interspeech.html); [active ASR learning, 2019](https://www.isca-archive.org/interspeech_2019/malhotra19_interspeech.html); and [MUCS, 2021](https://arxiv.org/abs/2104.00235). These are not presented as recent implementation evidence.

## Exact runtime and model boundary

The local artifact manifest and adapter registry are authoritative for deployed identities.

| Component | Inspected pin | Official capability and limit |
| --- | --- | --- |
| Nemotron native runtime | NeMo-Speech.cpp v0.1.0, `4f9676226f667d14608487df744f375db87127f8` | [ABI](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/4f9676226f667d14608487df744f375db87127f8/include/nemo_speech/asr.h), [RNNT decoder](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/4f9676226f667d14608487df744f375db87127f8/src/asr/decoders/rnnt_greedy_decoder.cpp): request phrases, one maximum boost across the list; empty list disables. No public acoustic gradient training or arbitrary acoustic adapter loading. |
| Nemotron checkpoint | `nvidia/nemotron-3.5-asr-streaming-0.6b@ea30d66debe3740a08b573244286791d423d6b3e` | Current Q8 SHA-256 `3fc991d3badad7277c11030a7519832cddaf2057aafed6d4b25147e953a070b1`. Use the [pinned configuration](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/blob/ea30d66debe3740a08b573244286791d423d6b3e/config.json), including its supported contexts. |
| sherpa-onnx | Rust crate 1.13.8, native `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf` | [Current official hotword guidance](https://k2-fsa.github.io/sherpa/onnx/hotwords/index.html) requires transducer modified beam search. It is not a generic Cohere/Qwen encoder-decoder switch. Exact pinned Parakeet hotword execution remains to be qualified; the app currently advertises no such support. |
| Cohere artifact | ONNX repository revision `156a470cf08eefe706a0004f3c52d9ee567ca7a0` | Selected completed-recording default; current app contract supports explicit English/French, without contextual phrases. Individual file hashes are in [artifacts.json](../models/artifacts.json). |
| Cleanup | `llama-cpp-2` 0.1.156, llama.cpp `e79e4bf660e19f2ad851e06c6913f7a8c5852621`; Qwen GGUF `8fea620810c4afa23dd6443f999a48574c1611a3` | Q4 SHA-256 `57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf`. Existing bounded prompting works with frozen weights. Future LoRA/export equivalence and model promotion remain unqualified. |

NVIDIA provides a [fine-tuning workflow](https://huggingface.co/blog/nvidia/fine-tuning-nemotron-35-asr) and [official notebook](https://github.com/nvidia-riva/tutorials/blob/main/asr-finetune-nemotron-3.5-asr-streaming-prompt.ipynb). They establish a GPU/BF16, language-conditioned full-checkpoint route, not in-place native GGUF learning. The notebook uses mutable upstream dependencies; a real experiment must pin them independently. The article's dataset totals and Bulgarian improvement narrative are internally inconsistent, so this report uses it for workflow capability only. Its general context table must not override the deployed checkpoint's configuration.

The pinned [native model guide](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/4f9676226f667d14608487df744f375db87127f8/docs/asr/models.md) documents compatible custom `.nemo` conversion. Thus architecture-preserving fine-tuning → GGUF is a plausible route; exact export, Q8 accuracy, native loading and rollback still need proof. New adapter layers, CTC heads or phoneme graphs may require category 2 runtime support in addition to category 3 training.

## How to establish improvement

Use the existing [ASR qualification protocol](../benchmarks/asr/README.md) and [supervision/split contract](self-improvement.md). Keep four evidence classes explicit: the paper's result; official model/runtime capability; measured local outcomes; and unverified hypotheses. This report supplies new research and code inspection, not new local quality measurements.

Compare on the same held-out audio:

1. Personalization disabled.
2. Existing vocabulary/examples enabled.
3. Approved alias and context selection, with the separate personalized output scored explicitly.
4. Native ASR phrase hints on/off within the same compatible adapter.
5. Later, a trained candidate versus its pinned base, both with matched memory settings.

Do not attribute a model switch or cleanup rewrite to acoustic learning. Build memory only from earlier reviewed sessions; group duplicate source hashes, overlapping audio, derivatives and revisions together. Reserve later sessions and fresh prompts for testing. Use a development split for thresholds and a separate final holdout; no test example may be remembered before scoring.

| Gate | Required evidence |
| --- | --- |
| Personal value | Lower paired entity-error/repeat-error rate on later reviewed sessions; active correction time measured in a separate counterbalanced usability study. Report review coverage. |
| False personalization | Entity precision and false substitutions on absent terms, real homophones, wrong project/language, irrelevant memories and contradictory examples. |
| Meaning | Names, numbers, dates, units, negation, uncertainty, valid repetition, quotes and explicit repairs; human review of added/removed information. |
| Language and script | English varieties, Hindi and Hinglish reported separately; retain exact WER/CER and a documented script/transliteration view. Do not normalize away wrong entities or language changes. |
| Context selection | Zero/one/eight/32 phrases, distractors, casing, spoken acronyms, short entity-only phrases and Unicode/identifier boundaries. Measure selection and total prompt cost. |
| Reliability | Disabled/deleted entries, stale saves, cancellation, repeated recordings, profile changes and new model versions; immutable raw fallback. |
| Performance | Cold/resident state, stage and stop-to-final p50/p95/p99 where sample size permits, RAM/VRAM, failures and sustained resource use. Extra replay/training jobs need resource limits. |
| Training retention | Later personal sessions plus general language/noise/microphone holdouts; original precision versus exported quantized artifact; load/promotion/rollback checks. |

Record application source hashes as well as Git revision while the worktree is uncommitted, exact model/runtime/artifact hashes, hardware/OS/driver/backend/precision/threads, corpus/license/language and profile revision. For streaming, retain chunk/right-context/endpoint settings, first partial, word emission, endpoint and stop-to-final definitions, partial stability, drops, cancellations, RAM/VRAM, power mode and thermal behavior. Never combine unlike hardware or corpus envelopes.

Choose numeric acceptance margins on development data before examining the final comparison. Report paired, session-grouped uncertainty intervals and preserve failures. A small pilot can identify failures; it cannot justify automatic promotion or prove a zero error rate. Existing general ASR ceilings alone are not evidence of personal improvement.

The approved local memory implementation now supports all six application-level areas described above. Personal benefit, false substitutions, review efficiency and intent usefulness still require later-session qualification. Acoustic training and accent detection remain deferred; no cloud API or new checkpoint is required for the implemented loop. Acceptance criteria and verification results remain in GitHub issue #34.
