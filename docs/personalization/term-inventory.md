# Term inventory and working-set selection

Vesper keeps a local **term inventory**: spellings that you asserted (spelling lists, text replacements) or authored (identifiers, package names and documentation headings in a workspace you opted into). It is separate from the 128-entry profile vocabulary. For each recording a pure **selector** picks a small, relevant **working set** from the inventory plus the profile vocabulary and hands it to whichever recognizer and correction step run.

Machine output is never a spelling source. ASR text, writer output, recurrence and rule output do not create or confirm inventory rows (see `crates/vesper-core/src/terms.rs`).

```mermaid
flowchart TB
  subgraph Sources["User-asserted and user-authored sources (read-only)"]
    TR["Text Replacements DB"]
    LD["LocalDictionary"]
    WS["Opted-in workspace root<br/>cspell · VS Code · JetBrains lists<br/>git-tracked code · manifests · doc headings"]
  end
  subgraph Worker["Background worker thread (QoS background, IOPOL_THROTTLE)"]
    G{"Gates: idle app · thermal ≤ fair ·<br/>no Low Power Mode · AC for workspace scans"}
    M["Deterministic miners<br/>tier-A parsers · tier-B admission"]
  end
  TR --> G
  LD --> G
  WS --> G
  G -->|pass| M
  G -.->|defer, retry ≤ 30 min| G
  M --> INV[("term_inventory<br/>spellings · spoken forms · counts per source/scope · status")]
  M --> LOG[("term_mining_runs<br/>counts and timings only")]
  UI["Settings: Words Vesper found<br/>Remove = tombstone · Forget source = pause"] --> INV
  subgraph Dictation["Per recording"]
    SNAP["Snapshot: profile + usable inventory rows"] --> SEL["select_working_set<br/>workspace → category → global · tier · evidence"]
    WSC["Resolved WorkspaceContext + project"] --> SEL
    SEL --> HINTS["hints ≤ 32 + hint_hash"]
    SEL --> REFS["references ≤ 16"]
    SEL -->|post-ASR, raw text| NORM["normalizations ≤ 64"]
  end
  INV --> SNAP
  HINTS -.->|"only through a qualified hook (hooks builder; unqualified)"| ASR["Recognizer"]
  NORM --> GATE["Correction gate decides"]
  REFS --> GATE
  ASRTEXT["ASR / writer / rule output"] -.-x|never a spelling source| INV
```

## Working-set selection

`vesper_core::select_working_set(terms, context, raw)` is deterministic and allocation-bounded. Its input is every usable inventory row plus `profile_terms(profile)` (manual words and confirmed corrections from the profile; unticked automatic entries contribute a spelling but no alias). The context is the resolved `WorkspaceContext`, the resolved project name, the dictation language and a clock.

Ranking, in order:

1. **Scope.** Rows scoped to the current workspace key or project come first. Global rows whose sources suit the receiving app's category come next: identifiers, manifests and project dictionaries suit code and terminal apps; documentation also suits documents and notes. Other global rows follow. Rows scoped only to *another* workspace are never offered, because the right term in the wrong scope is a hard negative.
2. **Tier.** Asserted (tier A) before authored (tier B).
3. **Evidence.** `ln(1 + independent evidence) × 0.5^(age / 30 days)`, where age counts from the last time mining saw the evidence grow. The profile's `usage_count` is deliberately not used: it counts raw ASR, including biased decodes.
4. Spelling, then ID, as a stable tie-break. Duplicate spellings (any case) collapse to the best-ranked row.

Outputs and caps (from `terms.rs`):

| Field | Cap | Contents |
| --- | --- | --- |
| `hints` | 32 phrases, 2,048 bytes | Top-ranked spellings for a qualified recognizer hook |
| `hint_hash` | — | `fnv1a64:<hex>` over the ordered hints; empty when there are no hints |
| `references` | 16 | Top-ranked single-token spellings with at least three letters that are **not** ordinary English words. The gate's near-miss repair uses them, so an ordinary word must never be a reference |
| `normalizations` | 64 | Post-ASR only: `(spoken form, spelling, term ID, tier)` for every spoken form that occurs as a whole-word n-gram (n ≤ 6) in the raw text |

Spoken forms come from `identifier_spoken_form`: camelCase, snake_case, kebab-case and dotted names split into lowercase words (`fooBarBaz` → `foo bar baz`, `HTTPServer` → `http server`, `vesper_core::terms` → `vesper core terms`). Digits stay attached (`k8s`, `utf8`), and forms longer than six words are dropped. Acronym letter forms are not generated. The selector only proposes normalizations; the [correction gate](correction-gate.md) decides whether each may apply.

## Sources and trust tiers

| Tier | Source (`TermSource`) | Where it is read | Scope |
| --- | --- | --- | --- |
| A, asserted | `text_replacement` | `~/Library/KeyboardServices/TextReplacements.db`, opened read-only; live rows of `ZTEXTREPLACEMENTENTRY` (`ZPHRASE`). The typed shortcut is not a spoken form and is dropped | Global |
| A | `spelling_dictionary` | `~/Library/Spelling/LocalDictionary` (one learned word per line) | Global |
| A | `project_dictionary` | Inside an opted-in workspace only: `cspell.json`, `.cspell.json`, `cspell.config.{json,yaml,yml}`, `cspell.{yaml,yml}`, `.vscode/cspell.json`, `package.json#cspell` (`words` plus `dictionaryDefinitions[].path` word lists inside the root), `.vscode/settings.json` `cSpell.words`/`cSpell.userWords`, `.idea/dictionaries/*.xml` `<w>` entries | That workspace |
| A | `vocabulary`, `correction` | The profile vocabulary, merged at selection time rather than copied into the inventory. Manual and confirmed entries contribute their taught aliases as spoken forms | The entry's project |
| B, authored | `identifier`, `manifest`, `documentation` | Git-tracked text files under an opted-in workspace root | That workspace |

Asserted values must look like a term: at most four words and 64 bytes, at least two letters, no `@`, no `://`, and no trailing sentence punctuation. That excludes "On my way!", addresses and URLs. cspell `flagWords` are errors, not spellings, and are ignored. JSONC comments and trailing commas are accepted. YAML is read only for the top-level `words` list and `dictionaryDefinitions` paths. Code-based cspell configurations (`cspell.config.js` and similar) are not executed or read.

### Tier-B admission (deterministic: no model, embedding or LLM)

The scanner lists files with `git ls-files` using a real git binary: Command Line Tools, Xcode or Homebrew. It never uses the `/usr/bin/git` shim, which can open an install dialog. A folder that is not a repository is skipped; there is no directory walk. It skips:

- vendor, build and generated folders (`node_modules`, `vendor`, `target`, `dist`, `build`, `.venv`, `site-packages`, `third_party` and similar)
- lockfiles and `*.min.*` files
- files over 512 KB, binary files, and files whose first 64 KB contain a line over 2,000 characters
- extensions outside a code, documentation (`md`, `rst`, `txt`, …) or configuration (`toml`, `yaml`) allowlist
- symlinks that resolve outside the root

Per file it collects candidate tokens. URLs are removed first, so hosts and user names in links are never mined.

- **Code:** every identifier-like token. Any token after a definition keyword (`fn`, `struct`, `class`, `def`, `function`, `interface`, `type`, …) is marked *defined*. The file stem is also marked defined.
- **Documentation:** all tokens, plus backticked spans with hyphens kept. Real headings (`#`–`######` followed by a space, outside fenced code blocks) are marked *heading*.
- **Manifests:** package and dependency names from `Cargo.toml`, `package.json` and `pyproject.toml` are marked *manifest*.

A token is a candidate only if it has one of these shapes:

| Shape | Example | Extra requirement |
| --- | --- | --- |
| Identifier: `_`/`-` separated, camelCase, or letters with digits | `sherpa_onnx`, `WorkingSet`, `k8s` | Defined, manifest or heading; at most five words |
| Capitalized non-dictionary word | `Nemotron`, `Qwen` | None |
| Acronym of 3–8 capitals | `GGUF`, `VAD` | Appears in a documentation file |
| Lowercase non-dictionary word of 4+ letters | `rusqlite`, `diarization` | Defined, manifest or heading |

"Dictionary" means the shared CMUdict-derived `ordinary_word` lexicon used by the correction gate, extended with regular inflections (`timings`, `retries`, `scoped`), common prefixes (`revalidate`, `denoising`) and, for lowercase words, two-word compounds (`runtime`, `frontend`). A short stoplist removes language keywords, ubiquitous library names (`and_then`, `Deserialize`) and generic tech words (`json`, `config`, `metadata`). Leading underscores, hash-like hex strings and chains of more than five words are rejected.

Spellings that differ only in case or separators (`vesper-core`, `vesper_core`, `VesperCore`) are grouped. A group is admitted when **at least two distinct files** contain any of its spellings and the shape evidence holds. The admitted surface is the **majority spelling** by file count. Its evidence count is the group's file count.

## Inventory rows and status

Each spelling has one row in `term_inventory`, carrying evidence entries per `(source, scope)`. Re-mining a source in a scope replaces only that source's entries in that scope, so two workspaces and the global sources never overwrite each other.

| Status | Set by | Selector use |
| --- | --- | --- |
| `active` | A row with any tier-A evidence | Hints, references, normalizations |
| `unconfirmed` | A tier-B-only row | Hints, references, normalizations (the gate applies authored terms only to non-word spans) |
| `suspended` | `suspend_inventory_term` (negative evidence such as a revert) | Not selected; rescans update counts but keep the status |
| `removed` | The user's **Remove** | Tombstone: never selected and never re-added by mining, in any letter case |

`DictationPersonalizationSnapshot` loads the usable rows with the profile in one read transaction (a damaged inventory yields an empty list, never a failed dictation). It carries three more fields:

- `workspace: WorkspaceContext`
- `inventory`
- `working_set`

After setup resolves the workspace and project, `select_working_set(language)` fills `working_set`. `post_asr_working_set(raw, language)` returns the post-ASR set for the correction gate, with hints and `hint_hash` copied from the pre-ASR set, so provenance describes what the decode actually received. A snapshot that was never selected reports empty hints. Lifecycle, bounds and deletion are described in [storage and privacy](storage-and-privacy.md#term-inventory).

## Background worker

`apps/desktop/src-tauri/src/term_worker.rs` runs one plain thread named `vesper-term-miner`. On start it lowers itself to background QoS (`pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND)`) with throttled disk I/O (`setiopolicy_np(IOPOL_TYPE_DISK, IOPOL_SCOPE_THREAD, IOPOL_THROTTLE)`). Both are implemented in `crates/vesper-desktop-platform/src/background.m`.

- **Schedule.** The first run is 2 minutes after launch, then one run per hour. If a gate defers a run, it retries every 5 minutes for up to 30 minutes of tolerance, then logs `deferred:<gate>` and waits for the next hour. Resuming a source wakes the worker early; the gates still apply.
- **Gates, checked at the start of a run:**
  - the app is closing
  - a runtime operation is busy (recording, transcribing, rewriting, installing, removing)
  - a background model download is running
  - a dictation is starting, recording, finishing or pasting
  - `ProcessInfo.thermalState` is worse than *fair*
  - Low Power Mode is on
- **Workspace scans need AC power.** On battery, or when the power source is unknown, the run mines the global tier-A files only and logs `partial:battery`.
- **Checks during a scan.** Before every file the worker re-checks:
  - cancellation
  - the busy and dictation gates, so a dictation start stops the scan within one file
  - the CPU budget

  Every 64 files it also re-checks thermal state, Low Power Mode and power source.
- **Budgets per run:**
  - 20 CPU-seconds, measured with the thread CPU clock
  - 2,000 files and 64 MB read across all workspaces
  - files over 512 KB skipped

  A pass that stops saves its cursor and resumes at the next run. A pass whose file fingerprint changed restarts. An unchanged workspace (same fingerprint as the last completed pass) costs one `git ls-files` plus one `stat` per tracked file, and reads nothing.
- **Database contention.** The worker opens the store with the 25 ms background lock limit and abstains rather than wait behind a foreground writer.
- **Run log.** Each run records its outcome (`completed`, `partial:*`, `deferred:*` or a fixed `failed:<category>`) together with counts and timings only:
  - wall time and CPU time
  - files read, bytes read and files skipped
  - workspaces scanned and unchanged
  - terms added, updated, dropped and in total

**Why a plain thread instead of NSBackgroundActivityScheduler.** The scheduler would add an Objective-C block bridge and completion-handler lifetime across the FFI boundary for little gain. The worker already needs app-specific gates that the system cannot see: dictation and model activity. It also needs per-file cancellation. The system's own deferral signals that matter here are applied directly: thermal state, Low Power Mode and power source. The thread spends nearly all of its life blocked on a condition variable. A later change could move the trigger to the scheduler without touching the miners.

**Opted-in roots (interim).** Tier-B mining runs only for roots the user opted into. Until the workspace store records that opt-in, roots come from the developer variable `VESPER_TERM_MINING_ROOTS`:

- entries are separated by `;`
- each entry is `scope=/path` or a bare `/path`
- a bare path's scope is its folder name, which matches a project of the same name

With no roots configured, only the global tier-A sources are mined.

## Privacy boundaries

- **Read:** Text Replacements (read-only SQLite), LocalDictionary, and, inside opted-in roots only, spelling-list files and git-tracked text files.
- **Not read:** the home folder at large, Mail, Messages, browser data, the screen or the clipboard. Mining requests no permission and needs no Full Disk Access.
- **Stored:** spellings, spoken forms, counts, opaque scope keys and folder-name labels. Never paths, URLs or file text. Cursors keep only a SHA-256 fingerprint, an index and candidate counts.
- **Not learned from:** machine output, which never creates, confirms or re-activates an inventory row.

## Limitations

- **Unmeasured benefit.** Recognition and correction gains are not measured here. Hints reach a recognizer only through a qualified hook, and the correction gate makes its own decisions.
- **Interim scopes.** Until the workspace store supplies opted-in roots and their salted keys, tier-B rows are scoped to the developer-configured scope string.
- **Main-window path.** Recordings started from the main window, and file transcriptions, load their snapshot lazily in the runtime. They currently get an empty pre-ASR working set, so no hints are used and the post-ASR hints report empty.
- **Admission is shape-based.** It admits many project identifiers made of ordinary words (`relative_path`). The gate refuses to rewrite ordinary-language spans into authored terms, so these mostly act as hints and references in code contexts.
- **Lexicon gaps.** English words missing from the lexicon (`reproducible`) can still pass as names or lowercase terms.
- **Late resume.** A pass interrupted by budget resumes only at the next hourly run.
