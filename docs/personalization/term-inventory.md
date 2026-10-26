# Term inventory and working-set selection

Vesper keeps a local **term inventory**: spellings that you asserted (spelling lists, text replacements) or authored (identifiers, package names and documentation headings in a workspace you opted into). It is separate from the 128-entry profile vocabulary. For each recording a pure **selector** picks a small, relevant **working set** from the inventory plus the profile vocabulary and hands it to whichever recognizer and correction step run.

Machine output is never a spelling source. ASR text, writer output, recurrence and rule output do not create or confirm inventory rows (see `crates/vesper-core/src/terms.rs`).

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
