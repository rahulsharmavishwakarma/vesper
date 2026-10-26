# Self-improving dictation system

Status: implemented on the local branch `feature/self-improving-dictation`. It is not published, and its tracking issue is pending the owner's approval to publish. This page describes the design, what each part does today and the measured results.

Vesper learns as a **system**, not as a model. Speech models are swappable, so what Vesper learns lives in an application-owned term memory. A small working set is handed to whichever recognizer is loaded, through that recognizer's own hook when the hook is qualified. Every model passes through the same text-level safety layer afterwards.

## Two loops

- **Memory loop (automatic).**
  - Terms, aliases and workspace links update continuously under fixed rules.
  - Sources are things the user wrote or asserted. Machine output (recognizer, writer, rules, recurrence) is never a spelling source and never positive evidence.
- **Policy loop (offline, approved).**
  - Thresholds, priors, list sizes and the near-miss mode change only through a versioned code change.
  - That change must pass the fixed [term evaluation](term-evaluation.md), which the learner can neither read nor modify.

## Architecture

```mermaid
flowchart TB
  subgraph Capture["Per dictation"]
    CR["Workspace resolver<br/>app category · hashed folder key · web host"] --> SEL
    INV[("Term inventory<br/>tier A asserted · tier B authored")] --> SEL["Working-set selector<br/>≤32 hints · ≤16 references · ≤64 normalizations"]
    VOC[("Profile vocabulary<br/>manual · learned aliases")] --> SEL
    SEL -->|"hints (qualified hook only)"| ASR["Any speech model"]
    ASR --> CL["Writer cleanup (optional)"]
    CL --> AL["Saved-correction aliases<br/>recognizer-keyed, cue-gated"]
    AL --> G["Correction gate<br/>exact normalizations · heard-plausibility<br/>near-miss repair"]
    G --> OUT["Displayed text<br/>stage authorship recorded"]
  end
  OUT -->|"user edit / Undo"| EV["Evidence rules<br/>asymmetric promotion · one-strike suspend"]
  EV --> BRK{"Policy breaker<br/>2 distinct rejections in 7 days"}
  BRK -->|tripped| PAUSE["Unconfirmed learned spellings paused<br/>until Resume or new policy"]
  EV --> VOC
  subgraph Background["Background learner (low priority, gated)"]
    MINE["Miners: Text Replacements · spelling dictionaries<br/>cspell · opted-in folders"] --> INV
  end
  subgraph Offline["Policy loop (versioned, owner-approved)"]
    EVAL["Fixed evaluator<br/>hard negatives · real speech · per model"] -.->|"qualifies hooks and defaults"| SEL
    EVAL -.-> G
  end
```

## Status and measured results (27 October 2026)

All results are **local measurements**. The positives are synthetic TTS, and the real-speech controls are small (92 FLEURS clips, 271 LibriSpeech utterances). Details and denominators are in the [term evaluation](term-evaluation.md).

| Component | Status in the app | Evidence |
| --- | --- | --- |
| Correction gate, exact normalizations | **On** for every model (English) | Cohere, at 32 workspace terms: positive recall 32.7% → 42.3%. Parakeet: 27.4% → 39.3%. 0 harmful edits. 0 added false alarms across 350 rendered and 2,595 text hard negatives and 92 real FLEURS clips. Replay over 725 real local transcripts: 0 changes after two rule fixes; before the fixes it made 5 wrong `ASR models` → `ASR_MODELS` style changes. |
| Near-miss repair | **On** (only with a working set; an undone repair suspends that spelling) | Adds about 5 points of recall on top (Cohere 42.3% → 47.6%, Parakeet 39.3% → 44.6%). With the correct workspace: 0 changes on 350 hard negatives and 92 FLEURS clips; 89 repairs on positives, 1 harmful. Real local history: 18 repairs, all judged correct (`GEMA` → Gemma, `collab` → Colab, `vasper` → Vesper, `Envidia` → NVIDIA, `DRization` → diarization); that history was also the development set. Under a wrong workspace's list: 1 harmful change in 92 FLEURS clips. Meets the predeclared criterion. |
| Recognizer hooks: Nemotron phrase bias, Qwen3-ASR prompt hotwords | **Disabled** for automatic hints | Both raise recall: Nemotron 25% → 60%, Qwen3-ASR 29% → 67%. Round 1: Nemotron at boost 2.0 inserts hint terms into 16–22% of hard negatives and 21–34% of FLEURS clips; Qwen3-ASR exceeds the 3% false-alarm bound at 32 hints. Round 2, with ordinary-word hints filtered: Qwen3-ASR raises FLEURS WER by 1.6 points and sometimes emits the hint list as text. Nemotron at boost 0.5 adds 7 points of exact recall with no FLEURS WER change, but cannot meet the per-lane 3% bound on 92 clips (its upper bound is 3.2% even at zero false alarms). The predeclared gate was not relaxed. The correction gate alone adds more (10–12 points). Each decode still records its hint provenance. After a hinted decode (the optional Settings list), near-miss repairs stay in shadow. |
| Policy 5 learning from edits | **On** | Stricter than policy 4. On the authored episode replay it gave fewer fixes than policy 4 (word errors 15 against 7, raw 16), with 0 harm in both. Local history holds 0 in-app edits, so this path has had no real input. |
| Automatic workspaces | **On** (folder/host keys) | Synthetic week, 20 seeds: precision 1,349/1,349, recall 59.7% (90.1% once a workspace is visible), median visibility after day 2–3. Live per-app document reading is **unmeasured**: running it means opening apps, which needs the owner's approval. |
| Term inventory and background miner | **On** (tier A from user lists; tier B only for opted-in folders) | Tier A yields 0 terms on the development Mac. Tier B, Vesper repo: 1,571 terms in 0.33 s CPU and 47 MB RSS; about 1 in 5 sampled terms is ASR-hard and project-specific. In-app worker cost, QoS effects and energy are unmeasured. |
| DPDFNet8, forced on | Unchanged (owner decision) | Lowered term recall in all six model and noise lanes (1.8–6.6 points case-insensitive) and raised non-target WER in five of six. Consistent direction, small samples. |

What did **not** work:
- Recognizer hints failed the predeclared gates for both hookable models in two rounds. Round 2 tried an ordinary-word filter and lower boosts.
- The writer model neither fixed nor inserted terms.
- Learning from edits had no real input, because the local history has no edits.

## Safety invariants

1. **Only user-authored evidence counts.** That means edits to displayed text, Undo, words the user added, and the user's own dictionaries and files. Recurrence of a term in recognizer output, unedited results, copied text and writer output are never positive evidence ([automatic dictionary](automatic-dictionary.md#independence-and-self-reinforcement)).
2. **Ordinary speech is not rewritten.** The [correction gate](correction-gate.md) refuses to change a span made of ordinary English words into a different word. A single lowercase ordinary word is never promoted to an automatic alias ([activation table](automatic-dictionary.md#automatic-activation)).
3. **One rejection suspends a rule. Two distinct rejections in a week pause all unconfirmed learned spellings** ([policy breaker](automatic-dictionary.md#policy-breaker)).
4. **Learned confusions are tied to the recognizer that produced them.** Vocabulary is model-independent ([recognizer-keyed aliases](automatic-dictionary.md#recognizer-keyed-aliases)).
5. **Hints are small and scoped.** A recognizer receives at most 32 workspace-relevant terms, and only through a hook that passed qualification for that model.
6. **Bias provenance.** Each decode records the hash of its hint list, so a term that appears *because* it was boosted cannot reinforce its own ranking.

## Related pages

- [Correction gate](correction-gate.md)
- [Automatic dictionary (policy 5)](automatic-dictionary.md)
- [Context selection and workspaces](context-selection.md)
- [Term inventory and background learner](term-inventory.md)
- [Term evaluation](term-evaluation.md)
- [Storage and privacy](storage-and-privacy.md)
