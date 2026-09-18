# Project working rules

## Track work with GitHub issues through gh

Use the GitHub CLI (`gh`) to create and maintain GitHub issues for **every repository-related job**, regardless of size. This includes bugs, tasks, features, enhancements, refactors, maintenance, tests, research, security work, performance work, build/release work, dependency changes, documentation-only work, and read-only repository investigations. GitHub Issues is the only task tracker. Do not create or maintain a local `issues/` folder, work-log tracker, or issue mirror. Put task descriptions, architectural proposals, approval status, progress, decisions, blockers, and completion results directly in the relevant GitHub issue.

Before beginning repository work, inspect the repository remote and search existing issues with `gh issue list` and `gh issue view`. Reuse a matching issue instead of creating a duplicate. If none exists, create one with `gh issue create` that explains the problem, proposed scope, completion criteria, and validation plan. Record architectural proposals and their approval status in the issue. A small job may use a concise issue, but it may not skip issue tracking.

Apply relevant labels when creating or reusing an issue, and keep them accurate if its scope changes. Use the existing repository taxonomy before creating labels. Assign at least one work-type label when applicable: `bug`, `enhancement` (including features), `documentation`, `task`, `maintenance`, `refactor`, `testing`, `research`, `security`, `performance`, `ci`, or `dependencies`. Add an existing area, platform, priority, or status label only when it truthfully helps routing. If a necessary reusable category is missing, create it with `gh label create`; do not create one-off labels or synonyms for an existing category.

Keep the issue updated using `gh issue edit` or `gh issue comment`: record the start or accepted scope, material progress, decisions, blockers, changed completion criteria, documentation changes, and final verification. Close it with `gh issue close` only when its completion criteria are met and the required documentation update below is complete. Explicitly record deferred, cancelled, or failed work instead of silently dropping it. Routine updates to an existing job reuse its issue and do not create duplicates.

Use explicit `--repo` arguments when the repository could be ambiguous. For multiline bodies or comments, write the exact content to a temporary file outside the source tree and use `--body-file`. Never include private audio, transcripts, credentials, tokens, or machine-specific sensitive paths. If GitHub access is unavailable, keep drafts only in temporary files and report the blocker without claiming that a remote issue was created or creating a local task tracker.

Creating or tracking an issue does not grant approval for architectural changes, staging, or commits. Follow the restrictions below.

## Documentation is part of completion

Every completed repository job must update the relevant canonical documentation under `docs/`. Documentation must describe the resulting behavior, design, operational procedure, evidence, limitations, or validation—not act as a second task tracker. Update an existing page when it already owns the topic; otherwise add a focused page and link it from the closest documentation index or README.

Documentation-only jobs still update the relevant `docs/` page and record that outcome in their GitHub issue. For internal maintenance with no user-visible behavior, update the closest development, architecture, validation, security, CI/release, performance, research, or operational document with the durable result and verification. Do not create per-task work-log pages merely to satisfy this rule.

Before declaring a job complete:

1. Verify the requested implementation, fix, investigation, or documentation result.
2. Update the relevant canonical file under `docs/`, including known limitations and deferred work.
3. Run validation appropriate to the change.
4. Confirm the issue labels still describe the completed scope.
5. Comment on the GitHub issue with changed files or components, documentation location, validation results, and remaining work.
6. Close the issue only when its completion criteria are satisfied; otherwise leave it open with an explicit status.

The contributor-facing version of this workflow is documented in [docs/project-workflow.md](docs/project-workflow.md).

## UI interaction design

Prefer familiar, direct interactions with the fewest necessary steps. Before introducing a new interaction or redesigning an unfamiliar control, check relevant platform guidance and one or two established applications; record the useful pattern and sources in the issue. Adapt behavior to this project's existing design system instead of copying another product's appearance or inventing a more complicated workflow.

For keyboard shortcuts, use one field that the user activates and then presses the desired key combination into, followed by Save. Do not require separate modifier buttons, a key dropdown, a customization panel, or an extra “Use shortcut” confirmation. Keep duplicate/conflict feedback beside the control, provide Escape cancellation and normal Tab navigation, and prevent editing keystrokes from triggering the underlying action.

Reuse shared controls, spacing, typography, wording, and motion across screens. Keep runtime/implementation details out of ordinary user flows. Validate the complete keyboard and pointer interaction, loading/error/disabled behavior, and narrow layouts. The canonical guidance and examples are in [docs/ui-design.md](docs/ui-design.md).

## Small local commits and attribution

The owner has explicitly authorized staging and local commits, superseding the earlier no-staging/no-commit restriction. Commit in small, coherent groups of at most 3–4 related implementation/configuration files. Directly related tests, fixtures and documentation may accompany their implementation; do not combine unrelated work into a large commit. Inspect the staged paths and diff before each commit, keep validation hooks enabled, and preserve ignored local files.

Reference the relevant GitHub issue in every commit message (for example, `Refs #50`) and record commit results in that issue. Referencing an issue does not mean it is complete: do not use automatic closing keywords unless its completion criteria are actually met.

Use the repository's existing Git author/committer identity. Do not override `--author`, add another author, or append AI/co-author attribution, `Co-authored-by`, or unsolicited sign-off trailers. Git's required author metadata still uses the owner's configured identity.

This permission covers local commits only. Do not push, create release tags, publish releases or change repository visibility unless separately authorized. These are persistent project preferences; no repeated staging/commit approval is needed within this scope.

## Approval before major changes

Ask the user for explicit approval before implementing a large architectural change or a substantial addition to this project. Present a concrete proposal explaining the problem, proposed approach, affected components, major tradeoffs, and validation plan. Wait for approval before making the dependent changes; silence is not approval.

This includes introducing a new subsystem, service, framework, or major dependency; changing inference ownership or model lifetimes; redesigning storage or data flow; and adding streaming transcription, cross-application insertion, or model training and promotion.

A general instruction such as “start fixing the issues” does not approve these architectural changes. Approval must cover the specific proposal. If the implementation materially expands beyond the approved scope, ask again for the expanded scope.

Small bug fixes, focused refactors that preserve architecture and behavior, tests, documentation, and read-only investigation may proceed without additional approval. Continue independent work while waiting. Do not repeat an approval request when the user has already explicitly approved the same proposal.

## Research-grounded inference work

Base streaming ASR, model-lifecycle, decoder, quantization, and accelerator work on primary research plus the exact model and runtime's official documentation. The canonical paper-to-engineering map is [docs/research-streaming-inference-papers.md](docs/research-streaming-inference-papers.md). Record the sources and the relevant model/runtime revisions in the GitHub issue and in any proposed ADR; an accepted paper or an architecture name is not evidence that this repository implements the technique.

For every inference proposal, distinguish these categories explicitly:

1. Frozen-weight application/runtime changes that the current public API supports, such as resident model ownership, fresh per-recording stream state, non-overlapping chunk delivery, queue bounds, finalization, and measurement.
2. Runtime-internal decoder or kernel work that needs support in the selected native runtime, such as a different transducer decoding loop, CUDA graphs, operator fusion, or a new quantized artifact format.
3. Training/checkpoint changes, such as FastEmit, alignment-restricted or minimum-latency training, multi-lookahead training, joint endpointing, distillation, multi-blank transducers, TDT, or a different encoder. Never present these as configuration switches for downloaded weights.

Keep four evidence classes separate in documentation and issues: paper result, official model/runtime capability, local measurement, and unverified hypothesis. Do not generalize published WER, speedup, memory, throughput, or algorithmic-latency numbers to a different checkpoint, quantization, language, backend, hardware, or application path.

Streaming experiments must record enough information to reproduce the result: application revision; model/runtime revision and artifact hashes; hardware, OS, driver, backend, precision, and thread settings; audio corpus and language; chunk/right-context and endpoint settings; cold or resident state; first-partial, word-emission, endpoint, stop-to-final, and stage timings; partial stability; WER/CER and meaning-preservation checks; queue drops; failures and cancellation/reset isolation; RAM/VRAM, power mode, and sustained thermal behavior. State the exact latency definition because paper-level algorithmic latency, real-time factor, endpoint latency, and user-visible latency are not interchangeable.

Use [benchmarks/asr/README.md](benchmarks/asr/README.md) for model qualification. A newer release is only a candidate: do not recommend it, mark it executable, or change a default until its architecture-specific adapter and pinned artifacts run the same held-out manifests. Compatibility with the requested language, application mode, actual backend, duration, automatic-language behavior, code-switching, and singing is a hard filter. Rank comparable local evidence ahead of publisher claims, preserve failures in aggregates, and never combine incompatible hardware/corpus/precision envelopes.

For independent research questions, use parallel agents when slots are available. Divide the work by non-overlapping evidence area, require primary papers and official implementations, and have the primary agent verify citations, reconcile conflicting claims, and produce one DRY synthesis. Agent output is research input, not project truth by itself.

These research rules do not waive the approval requirement above. In particular, implementing persistent inference ownership or live streaming still requires a concrete proposal and explicit user approval.
