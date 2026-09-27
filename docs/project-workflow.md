# Repository work tracking and documentation

Every repository-related job is tracked in GitHub Issues through the GitHub CLI. This applies to bugs, tasks, features, refactors, maintenance, tests, research, security, performance, dependencies, build/release work, documentation changes, and read-only repository investigations. A job's size does not exempt it.

GitHub Issues is the only task tracker. Project documentation records current behavior, decisions, procedures, evidence, and limitations; it does not mirror issue status or become a local backlog.

Use **Vesper** in product-facing issue titles/bodies, templates and the linked
project's display title/readme. Current technical references use the Vesper
namespaces; keep live repository links and explicit compatibility aliases intact.
Naming-only edits do not alter issue approvals, status,
test outcomes or completion criteria. Preserve quoted old names and dated
evidence as historical, with a current-name note where needed; see
[product naming and compatibility](development.md#product-name-and-compatibility).

## Required lifecycle

1. Inspect the configured Git remote and search open and closed issues with explicit `--repo` arguments when repository identity could be ambiguous.
2. Read the matching issue and reuse it. If none exists, create one containing the problem, proposed scope, completion criteria, validation plan, and the documentation affected by the work.
3. Apply the relevant work-type label and any useful existing routing labels. Keep them accurate when scope changes.
4. Record any architectural proposal and its approval status. Creating an issue does not itself authorize a major architectural change.
5. Update the issue when scope, decisions, progress, blockers, or completion criteria change.
6. Before the first repository edit, create and check out a new feature branch from the freshly fetched default branch. Preserve existing local work separately when needed. Implement or investigate there, then prepare small local commits under the owner's standing authorization and the rules below.
7. Update canonical documentation alongside the implementation, in the same change. Prefer an existing owner page; otherwise add a focused page and link it from the nearest index or README. Include related docs in the implementation commit when practical.
8. Run proportionate validation and publish the result, documentation location, limitations, and deferred work to the issue.
9. Confirm labels still represent the completed scope, then close only when every completion criterion—including documentation—is complete. Leave incomplete work open with an explicit status.

## Feature branches and pull requests

Use a new task-specific feature branch before editing code, documentation or
repository rules. Do not develop or commit on `main`, and do not wait until PR
creation to move the work to a branch. Inspect the current branch, working tree,
local-only commits and worktrees before choosing the base.

For a clean checkout with no local-only work, the normal start is:

```sh
git fetch origin main
git switch main
git merge --ff-only origin/main
git switch -c feature/short-task-name
```

Branch names use `<type>/<short-kebab-description>`. Use `feature/` for user-facing
capabilities, `fix/` for defects, `chore/` for maintenance, `docs/` for
documentation-only work, `test/` for test or benchmark infrastructure,
`refactor/` for behavior-preserving restructuring, `perf/` for performance work,
`ci/` for automation, and `release/` for release preparation. Do not prefix
branches with an agent or contributor name.

Use the actual default branch if it is not `main`. When the checkout contains
other work, preserve it on a dedicated branch and in a recoverable snapshot, then
start a fresh feature branch or isolated worktree from the fetched base. Ordinary
`git stash` saves tracked changes only; untracked files require explicit
preservation, and ignored local state must also remain intact. Never delete a
file merely because it blocks checkout or rebase.

Before opening the PR, inspect the entire commit list and diff against its base,
including earlier commits inherited by the branch. Publish only the requested
scope, keep signed commits and hooks enabled, and use the
[PR checklist](../.github/PULL_REQUEST_TEMPLATE.md). The PR head must be the task's
feature branch and its base the default branch. Record both in the tracking
issue. Branch creation does not grant publication or architectural approval.

After merging, fetch and verify the actual merged content. Rebase and squash
merges can change commit IDs, so ancestry alone may not prove whether the work
landed. Fast-forward a clean local `main` when possible. A divergent `main` needs
its local-only commits preserved and reconciled first; changing `pull.ff` does
not resolve that divergence. An applied stash also remains in the stash list.
Do not blindly reapply already-merged changes or force away remaining work.
Retain recovery references until restoration is verified, and continue unfinished
work on its own feature branch.

The shared-checkout handoff state is `main`, provided switching preserves all
existing work. Each incoming agent checks the state and creates/checks out its
own feature branch before editing. Do not discard or silently stash another
agent's changes to obtain a clean handoff. Use isolated worktrees for concurrent
tasks when a shared branch switch would disrupt active work, and report a blocked
handoff rather than forcing it.

## Local commit convention

The owner authorizes local staging and commits. Use at most 3–4 related
implementation/configuration files per commit; their directly related tests,
fixtures and documentation may accompany them. Review staged paths and content,
keep hooks enabled, and leave ignored screenshots, generated builds, recordings,
weights and local tool state outside Git. A first baseline assembled in small
commits may depend on later commits in that baseline series; the completed
series is the reproducible validation point.

Every commit references its GitHub issue using `Refs #NUMBER` or a title suffix.
Record resulting commit identifiers and validation in the issue. Use `Refs`
while completion criteria remain open; a checkpoint must not silently close
unfinished research or features. GitHub will resolve local commit links only
after the owner separately authorizes a push.

Use the existing configured Git identity without an author override, additional
authors, AI attribution, co-author trailers or unsolicited sign-offs. Ordinary
Git author/committer metadata remains required. Local commit permission does not
authorize a push, release tag, publication or visibility change. These preferences
are also recorded in `AGENTS.md` for future repository sessions.

Every new commit must retain the configured GPG signature. Verify it with
`git verify-commit HEAD`; never disable signing or create an unsigned fallback.
If the key is locked or the passphrase prompt is unavailable, preserve the staged
batch and wait for the owner to unlock the key locally.

## Releasing a branch held by another worktree

If checkout reports that a branch is already used by another worktree, inspect
`git worktree list` and that worktree's status first. When it contains pending
work, switch it to a new preservation branch with
`git -C <holding-worktree> switch -c <preservation-branch>`. The new branch starts
at the same commit and retains staged and unstaged changes in that worktree.
The original branch is then available for checkout in the shared workspace.

Verify that the pending diff, stash entries and unrelated files are unchanged.
Uncommitted changes remain in the holding worktree; the preservation branch alone
does not store them. Keep that worktree until the work is saved. Do not force-remove
it or apply or drop stashes to resolve this checkout error. Releasing a branch
does not reconcile divergence with its remote; that is a separate operation.

## Label taxonomy

Every issue should have at least one truthful work-type label. Use `enhancement` for both features and enhancements so the repository does not maintain duplicate concepts.

| Label | Use for |
| --- | --- |
| `bug` | Incorrect or regressed behavior |
| `enhancement` | New or improved user-visible capability |
| `documentation` | Documentation as a primary deliverable |
| `task` | Scoped work that does not fit a more specific type |
| `maintenance` | Repository hygiene and routine upkeep |
| `refactor` | Internal restructuring without intended behavior changes |
| `testing` | Test coverage, fixtures, benchmarks, or validation infrastructure |
| `research` | Evidence gathering and synthesis |
| `security` | Security, privacy, secrets, or supply-chain work |
| `performance` | Latency, throughput, memory, power, or efficiency work |
| `ci` | Continuous integration, builds, containers, or releases |
| `dependencies` | Dependency and lockfile maintenance |

Combine work-type labels when the deliverables genuinely span categories, such as `performance` plus `research`. Add an existing area, platform, priority, or status label only when it improves routing. Prefer the existing taxonomy; create a missing label only when it will be reused, and never add an issue-specific synonym.

## Architecture diagram workflow

Mermaid fenced blocks in the owning `docs/` page are the reviewable source for
architecture diagrams. Use a component/data-flow diagram for ownership and data
boundaries, or a sequence diagram when ordering, cancellation and asynchronous
callbacks determine correctness. Update the existing diagram with its related
implementation; avoid another diagram for a change that leaves these boundaries
unchanged. [Architecture](architecture.md) owns the overview, and feature guides
own their detailed flows.

Distinguish current behavior from proposals and failed qualification gates.
Include the important raw-output, storage, permission, model-lifetime and
failure boundaries rather than every function. A proposed diagram does not
approve implementation; the issue retains the architectural decision and
approval record.

For visual review, use the official [Excalidraw MCP server](https://github.com/excalidraw/excalidraw-mcp)
when connected. Read its element reference before drawing and confirm the live
schema: the reviewed `create_view` API accepts an `elements` JSON string, not
Mermaid text. The separate official [Mermaid-to-Excalidraw converter](https://github.com/excalidraw/mermaid-to-excalidraw)
can produce elements for supported syntax. Unsupported conversions need an
equivalent element layout checked against the source; do not silently drop
edges or treat an image fallback as an editable diagram.

Render the Mermaid source and inspect labels, arrow direction, boundary labels
and clipping. Inspect the Excalidraw view too when one is produced. If a visual
artifact is retained, keep its editable `.excalidraw` file beside the owning
source and update them together. An expiring MCP checkpoint or shared URL is
not a substitute for repository documentation. Hosted rendering may receive
only public/synthetic architecture labels, never user audio, transcripts,
document content, sensitive paths or credentials.

When MCP is unavailable, record that limitation and the Mermaid validation
performed in the issue. No global MCP configuration change or product runtime
dependency is required by this documentation workflow.

## GitHub CLI patterns

Use `gh`, not hand-written REST requests or a local issue directory:

```sh
gh issue list --repo OWNER/REPOSITORY --state all --search "short topic"
gh issue view NUMBER --repo OWNER/REPOSITORY
gh issue create --repo OWNER/REPOSITORY --title "..." --body-file /tmp/issue-body.md
gh issue edit NUMBER --repo OWNER/REPOSITORY --add-label bug
gh issue comment NUMBER --repo OWNER/REPOSITORY --body-file /tmp/progress.md
gh issue close NUMBER --repo OWNER/REPOSITORY --comment "Completion criteria and documentation are complete."
```

For multiline content, write the exact body to a temporary file outside the source tree and pass `--body-file`. Never publish private audio, transcripts, credentials, tokens, or sensitive machine-specific paths. If GitHub is unavailable, keep the intended body only in a temporary file, report the blocker, and do not claim a remote update occurred.

## Documentation completion standard

Documentation should explain what future users and maintainers need to know after the job:

| Change area | Typical canonical documentation |
| --- | --- |
| User-visible behavior | `docs/user-guide.md`, owning feature guide, relevant README overview/setup changes, and limitations |
| Architecture or data flow | `docs/architecture.md`, `docs/internals.md`, and an ADR when required |
| Model/runtime behavior | Model guide, inference-performance evidence, benchmark contract, and provenance |
| Bug fix | Owning behavior page plus regression/validation evidence |
| Testing or CI | `docs/development.md`, `docs/validation.md`, or `docs/ci-and-releases.md` |
| Security or privacy | `SECURITY.md` and the closest security/privacy document under `docs/` |
| Research | Research synthesis with primary sources, evidence class, implementation boundary, and issue link |
| Documentation-only work | The corrected canonical page and any necessary index links |

Do not create a per-issue Markdown file or chronological local work log solely to satisfy documentation completion. The GitHub issue owns task state; `docs/` owns durable project truth.

Documentation is part of each feature, bug fix, task, test, refactor and
maintenance change, not a separate cleanup promised for later. Correct behavior
descriptions and troubleshooting when fixing bugs; update examples/defaults when
changing configuration or commands; record operational and validation changes
even when the user interface stays the same. Preserve historical measurements
as dated evidence rather than rewriting them to describe a newer implementation.

The root README is the product entry point. Keep purpose, capabilities, actual
download availability, first-run steps, privacy and user/contributor links there.
Keep runtime details and benchmark/research notes in their owning guides. Check
current UI wording, commands, links, platform support and release status before
claiming completion. Unpublished installers and pending licenses must stay
clearly identified. The issue's completion comment names the updated pages and
their validation; an issue comment alone does not replace the documentation.

## UI changes

Follow [UI interaction design](ui-design.md) when adding or changing controls. For unfamiliar interactions, check relevant platform guidance and established products, record the useful pattern in the issue, and choose the fewest necessary user steps. Shortcut assignment uses a direct key-recording field followed by Save, with validation and an exit path.

## Relationship to other project rules

- Major architectural changes still require the user's explicit approval after a concrete proposal. An issue is necessary but not sufficient approval.
- Local commits follow the owner's small-commit and attribution convention above; remote publication requires separate authorization.
- Research-grounded inference work must preserve paper results, official capabilities, local measurements, and unverified hypotheses as separate evidence classes.
- A completed code change without documentation is incomplete; a documentation update without issue status and validation is also incomplete.

This workflow was formalized under [GitHub issue #44](https://github.com/rahulsharmavishwakarma/transcribe/issues/44).
