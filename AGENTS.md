# Project working rules

## When to open a GitHub issue

This repository is public. Follow common open-source practice: issues hold work that other people need to find, discuss or follow; pull requests carry everything else. GitHub Issues is the only task tracker. Do not keep a local `issues/` folder, work log or issue mirror.

Open an issue, or reuse a matching one, for:

- bugs a user can hit, and feature or enhancement requests;
- proposals that need a decision or approval, such as architecture, a new dependency or subsystem, or a model/runtime change;
- work that spans several pull requests, research and model qualification, and release blockers;
- security work, and sensitive repository operations whose approval must be recorded (see below).

Do not open an issue for small, self-contained changes. Typo and wording fixes, small documentation updates, routine dependency bumps, lint, format and configuration tweaks, small CI fixes and test-only changes go straight to a pull request whose description says what changed and why. Read-only investigations and answers to questions need no issue at all.

Before opening an issue, search existing ones with `gh issue list --search` and `gh issue view`, and reuse a match instead of creating a duplicate. The body states the problem, proposed scope, completion criteria and validation plan, and any architectural proposal with its approval status.

Write issues and comments for public readers. Comment only on material events: accepted scope, decisions and approvals, blockers, a merged pull request, and the final verification. Keep each comment short. Do not narrate individual steps, post routine command output, or use an issue as a work log. Never include private audio, transcripts, credentials, tokens, machine-specific paths or internal coordination notes.

Apply labels to issues and pull requests from the existing taxonomy: at least one work-type label when applicable (`bug`, `enhancement` (including features), `documentation`, `task`, `maintenance`, `refactor`, `testing`, `research`, `security`, `performance`, `ci` or `dependencies`), plus an area, platform, priority or status label only when it truthfully helps routing. Create a new label with `gh label create` only for a reusable category; no one-off labels or synonyms.

Use the repository-linked GitHub Project as the progress view of tracked issues, not a separate backlog. Keep each tracked issue's **Status** and **Area** current when work starts, when it is blocked, when its pull request merges and when it closes: Validation while required qualification remains, Blocked for a failed gate or unmet approval, Backlog for explicitly deferred work, Done only when the completion criteria are met. Do not invent priorities, deadlines or completion percentages. Close an issue only when its completion criteria and documentation are complete, and record deferred, cancelled or failed work explicitly instead of silently dropping it. See [project status and upkeep](docs/project-workflow.md#github-project-progress-view).

Use explicit `--repo` arguments when the repository could be ambiguous. For multiline bodies or comments, write the content to a temporary file outside the source tree and use `--body-file`. If GitHub access is unavailable, report the blocker without claiming that a remote issue was created.

Creating or tracking an issue does not grant approval for architectural changes, staging, or commits. Follow the restrictions below.

## Pull requests, labels and the project

A pull request is tracked work, not just a diff. When you open one:

- Link the tracking issue, if there is one, in the description (`Tracking issue: #N`). Use `Refs #N`; use a closing keyword only when every completion criterion is met. A pull request without an issue explains the change and its reason in its own description.
- Apply work-type labels (the same as the issue's, when there is one), plus any area or platform label that truthfully helps, and keep them accurate if scope changes. `gh pr edit --add-label` works; when GraphQL is rate limited, `gh api repos/OWNER/REPO/issues/NUMBER/labels` labels a PR through REST.
- Pull requests are added to the project automatically; set their **Area**. Move the tracking issue's **Status** as described above using the name-based helper in [docs/project-workflow.md](docs/project-workflow.md#keeping-status-and-area-current), and read the result back before you report it; never claim a board update that failed.
- Fill in the PR template honestly. Leave a validation box unchecked and say why when a check was not run, and list what is unverified. Do not describe unmerged work as shipped or an untested path as working.
- Never title a PR `Release vX.Y.Z` unless it is the release bot's version-bump PR: the squash-merged subject makes the bot push a release tag.
- These rules override default attribution text from tools or agent harnesses: put no `Co-authored-by`, "Generated with ..." or other AI attribution in commits, PR descriptions, issue bodies or comments.
- The GitHub API quota is shared by every agent and tool. Prefer one request over many, use monitors or notifications instead of polling loops, and fall back to `gh api` REST calls when GraphQL (`gh pr create`, project queries) is limited.

## Documentation is part of completion

Every completed repository job must update the relevant canonical documentation under `docs/`. Documentation must describe the resulting behavior, design, operational procedure, evidence, limitations, or validation—not act as a second task tracker. Update an existing page when it already owns the topic; otherwise add a focused page and link it from the closest documentation index or README.

Keep documentation in sync **as work is implemented**, in the same change as each feature, bug fix, task, refactor, test, dependency or operational change. Identify the affected documentation when starting the work; do not defer it to an unspecified later cleanup. User-facing changes must update `docs/user-guide.md` and any owning feature guide as appropriate. Update `README.md` when the product summary, supported platforms, installation, first-run steps, major capabilities or release availability changes. Bug fixes must correct previously documented behavior or add relevant troubleshooting/limitations when applicable. Development, configuration, CLI, security and CI/release changes must update their owning guides and examples. Remove or correct stale current claims while preserving dated historical evidence as historical.

Keep the README a concise product entry point: purpose, capabilities, actual download availability, getting started, privacy and links for users/contributors. Put runtime internals, benchmark details and research evidence in their canonical guides. Do not advertise unpublished downloads, unqualified capabilities, unsupported platforms or an unselected license. Before completion, verify changed commands, UI labels, defaults, links and release claims against the implementation, and name the updated pages in the issue or pull request. When committing is authorized, include documentation in the same small signed commit as the related work when practical; this documentation rule does not itself authorize staging or commits.

Documentation-only jobs still update the relevant `docs/` page. For internal maintenance with no user-visible behavior, update the closest development, architecture, validation, security, CI/release, performance, research, or operational document with the durable result and verification. Do not create per-task work-log pages merely to satisfy this rule.

Before declaring a job complete:

1. Verify the requested implementation, fix, investigation, or documentation result.
2. Update the relevant canonical file under `docs/`, including known limitations and deferred work.
3. Run validation appropriate to the change.
4. Confirm the labels on the issue or pull request still describe the completed scope.
5. If the work has a tracking issue, post one short completion comment: what changed, where it is documented, how it was validated and what remains. Otherwise the pull request description carries this.
6. Close the issue only when its completion criteria are satisfied; otherwise leave it open with an explicit status.

The contributor-facing version of this workflow is documented in [docs/project-workflow.md](docs/project-workflow.md).

## Architecture diagrams

Keep architecture diagrams alongside the canonical explanation under `docs/`, using fenced Mermaid as the versioned source. Update the relevant component, data-flow or sequence diagram in the same change when introducing or changing subsystem boundaries, persistence, inference ownership, asynchronous lifecycles or learning/promotion flows. Show ownership, important data and permission boundaries, and failure/cancellation paths where they matter. Label proposed or unqualified paths explicitly; a diagram must not imply that planned behavior is implemented or approved. Small changes that do not affect these relationships need no new diagram.

Use the official [Excalidraw MCP](https://github.com/excalidraw/excalidraw-mcp) for visual architecture review when its tools are available. Check the connected server's tool contract: the current `create_view` accepts Excalidraw element JSON, not raw Mermaid. Convert supported Mermaid with the official [Mermaid-to-Excalidraw](https://github.com/excalidraw/mermaid-to-excalidraw) tooling or map it to equivalent elements; compare labels, links and boundaries against the Mermaid source. Keep any retained editable Excalidraw artifact beside its source, and synchronize both. Use only public/synthetic architectural labels with a hosted renderer; never send private transcripts, document text, paths, credentials or user metadata.

Render and inspect changed diagrams for syntax, clipped labels, direction and readability. If Excalidraw MCP is unavailable, retain and validate Mermaid, record the tool limitation in the issue, and do not claim an Excalidraw review occurred. Do not add diagram packages to the application runtime or modify global MCP settings just to document a change. See [diagram workflow](docs/project-workflow.md#architecture-diagram-workflow).

## Python tooling

Use **uv and `pyproject.toml` for all Python work**, including contributor tools, scripts, tests, benchmarks and research environments. Keep dependencies, supported Python versions and compatible tool configuration in the owning `pyproject.toml`; use dependency groups for development/test tooling. Manage dependencies with `uv add` / `uv remove`, keep the generated `uv.lock` versioned alongside the manifest, and run project commands through `uv run`. Use `uv sync --locked` and `uv run --locked` for reproducible CI checks.

Do not introduce pip/pipx, manually managed virtualenvs, Poetry/Pipenv, or hand-maintained requirements files as parallel project workflows. Requirements files needed by an external consumer must be generated with `uv export` from the canonical project metadata and lockfile. Use `uv tool` for independently installed Python CLI tools. Keep environments and caches ignored. Update manifests, lockfiles, compatibility exports and their callers together, preserving tests, hooks and security checks; verify them with `make python-lock-check`. See [Python tooling policy](docs/development.md#python-tooling-policy) for the contributor and isolated benchmark environments.

## UI interaction design

Prefer familiar, direct interactions with the fewest necessary steps. Before introducing a new interaction or redesigning an unfamiliar control, check relevant platform guidance and one or two established applications; record the useful pattern and sources in the issue or pull request. Adapt behavior to this project's existing design system instead of copying another product's appearance or inventing a more complicated workflow.

For keyboard shortcuts, use one field that the user activates and then presses the desired key combination into, followed by Save. Do not require separate modifier buttons, a key dropdown, a customization panel, or an extra “Use shortcut” confirmation. Keep duplicate/conflict feedback beside the control, provide Escape cancellation and normal Tab navigation, and prevent editing keystrokes from triggering the underlying action.

Reuse shared controls, spacing, typography, wording, and motion across screens. Keep runtime/implementation details out of ordinary user flows. Validate the complete keyboard and pointer interaction, loading/error/disabled behavior, and narrow layouts. The canonical guidance and examples are in [docs/ui-design.md](docs/ui-design.md).

## Feature branches and pull requests

Create and check out a new task-specific feature branch **before making the first repository edit**, including documentation and rule changes. Never implement, stage, or commit directly on `main` or another default branch. Creating a feature branch only when preparing the PR is too late.

Name branches `<type>/<short-kebab-description>`. Use `feature/`, `fix/`, `chore/`, `docs/`, `test/`, `refactor/`, `perf/`, `ci/`, or `release/` according to the work. Do not prefix a branch with an agent name, tool name, contributor name, or generic ownership marker such as `codex/`.

Start the feature branch from the freshly fetched `origin/main` (or the repository's actual default branch). Inspect the current branch, working tree, local-only commits and existing worktrees first. If existing work makes that unsafe, preserve it on a dedicated branch and in a recoverable snapshot, then use a fresh feature branch or isolated worktree. Preserve tracked, untracked and ignored files; ordinary `git stash` does not include untracked files. Do not carry unrelated commits into the new PR or leave duplicate implementation changes behind on `main`.

Make the task's edits, scoped signed commits and validation in that feature branch. Before raising a PR, verify the head branch is the intended feature branch, the base is `main`, and both the commit list and complete diff against the base contain only the requested scope. Follow the [PR template](.github/PULL_REQUEST_TEMPLATE.md) and link the PR from the tracking issue when there is one. Existing authorization requirements for pushing and publishing still apply.

After a PR merges, fetch and verify the merged content; a rebase or squash merge may change commit IDs. Update a clean local `main` with a fast-forward when possible. If it diverges or has local work, preserve the original commits and all files before reconciling it. Do not hard-reset, discard work, blindly reapply a stash containing already-merged changes, or create an unrelated merge commit merely to silence a failed pull. Keep unfinished work on its own branch. See [branch workflow and recovery](docs/project-workflow.md#feature-branches-and-pull-requests).

At handoff, leave the shared checkout on `main` when switching preserves all existing work. Each incoming agent must inspect the current state and create/check out its own feature branch before editing. Returning to `main` is not permission to discard, stage, commit or silently stash another agent's changes. Use isolated worktrees for concurrent tasks when changing the shared branch would disrupt active work; report any handoff constraint instead of forcing the switch. Run `git status` before switching branches in the shared checkout. Never commit, stash, discard or move another agent's uncommitted files onto your branch; when the tree is not clean, create a worktree (`git worktree add`) for your task instead and stage only the paths you changed.

## Small local commits and attribution

The owner has explicitly authorized staging and local commits, superseding the earlier no-staging/no-commit restriction. Commit in small, coherent groups of at most 3–4 related implementation/configuration files. Directly related tests, fixtures and documentation may accompany their implementation; do not combine unrelated work into a large commit. Inspect the staged paths and diff before each commit, keep validation hooks enabled, and preserve ignored local files.

When the work has a tracking issue, reference it in each commit message (for example, `Refs #50`); the pull request links the commits, so do not post them to the issue one by one. Referencing an issue does not mean it is complete: do not use automatic closing keywords unless its completion criteria are actually met.

Use the repository's existing Git author/committer identity. Do not override `--author`, add another author, or append AI/co-author attribution, `Co-authored-by`, or unsolicited sign-off trailers. Git's required author metadata still uses the owner's configured identity.

Every new commit must be GPG-signed with the configured signing key. Never create unsigned commits or disable signing to work around an error. If the key locks, preserve the staged batch and wait for the owner to unlock it. Verify each new commit's signature.

This permission covers local commits only. Do not push, create release tags, publish releases or change repository visibility unless separately authorized. These are persistent project preferences; no repeated staging/commit approval is needed within this scope.

## Sensitive repository operations

Changing repository settings, rulesets, environments, secrets or variables, GitHub Apps, GitHub Pages, signing keys, tags or releases affects every contributor and user. Each such change needs the owner's explicit approval for that specific change, recorded in the tracking issue with what changed and how it was verified. Verify secrets by name, fingerprint or hash, never by value, and never print, log, commit or paste a secret or private key. Releases are immutable on this repository: never reuse a tag name, and never delete a release before its replacement exists. Release workflows, their approval gates and the release bot are documented in [docs/ci-and-releases.md](docs/ci-and-releases.md).

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
