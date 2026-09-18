# Repository work tracking and documentation

Every repository-related job is tracked in GitHub Issues through the GitHub CLI. This applies to bugs, tasks, features, refactors, maintenance, tests, research, security, performance, dependencies, build/release work, documentation changes, and read-only repository investigations. A job's size does not exempt it.

GitHub Issues is the only task tracker. Project documentation records current behavior, decisions, procedures, evidence, and limitations; it does not mirror issue status or become a local backlog.

## Required lifecycle

1. Inspect the configured Git remote and search open and closed issues with explicit `--repo` arguments when repository identity could be ambiguous.
2. Read the matching issue and reuse it. If none exists, create one containing the problem, proposed scope, completion criteria, and validation plan.
3. Apply the relevant work-type label and any useful existing routing labels. Keep them accurate when scope changes.
4. Record any architectural proposal and its approval status. Creating an issue does not itself authorize a major architectural change.
5. Update the issue when scope, decisions, progress, blockers, or completion criteria change.
6. Implement or investigate, then prepare small local commits under the owner's standing authorization and the rules below.
7. Update the canonical documentation under `docs/`. Prefer an existing owner page; otherwise add a focused page and link it from the nearest index or README.
8. Run proportionate validation and publish the result, documentation location, limitations, and deferred work to the issue.
9. Confirm labels still represent the completed scope, then close only when every completion criterion—including documentation—is complete. Leave incomplete work open with an explicit status.

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
| User-visible behavior | Feature guide, root README link, and limitations |
| Architecture or data flow | `docs/architecture.md`, `docs/internals.md`, and an ADR when required |
| Model/runtime behavior | Model guide, inference-performance evidence, benchmark contract, and provenance |
| Bug fix | Owning behavior page plus regression/validation evidence |
| Testing or CI | `docs/development.md`, `docs/validation.md`, or `docs/ci-and-releases.md` |
| Security or privacy | `SECURITY.md` and the closest security/privacy document under `docs/` |
| Research | Research synthesis with primary sources, evidence class, implementation boundary, and issue link |
| Documentation-only work | The corrected canonical page and any necessary index links |

Do not create a per-issue Markdown file or chronological local work log solely to satisfy documentation completion. The GitHub issue owns task state; `docs/` owns durable project truth.

## UI changes

Follow [UI interaction design](ui-design.md) when adding or changing controls. For unfamiliar interactions, check relevant platform guidance and established products, record the useful pattern in the issue, and choose the fewest necessary user steps. Shortcut assignment uses a direct key-recording field followed by Save, with validation and an exit path.

## Relationship to other project rules

- Major architectural changes still require the user's explicit approval after a concrete proposal. An issue is necessary but not sufficient approval.
- Local commits follow the owner's small-commit and attribution convention above; remote publication requires separate authorization.
- Research-grounded inference work must preserve paper results, official capabilities, local measurements, and unverified hypotheses as separate evidence classes.
- A completed code change without documentation is incomplete; a documentation update without issue status and validation is also incomplete.

This workflow was formalized under [GitHub issue #44](https://github.com/rahulsharmavishwakarma/transcribe/issues/44).
