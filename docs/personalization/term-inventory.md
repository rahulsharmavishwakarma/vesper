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
