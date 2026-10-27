# Contributing to Vesper

Thank you for improving Vesper. Contributions should keep speech processing local, preserve original user data, and make collection or destructive behavior explicit.

## Before you start

Open an issue for bugs, feature requests, proposals that need a decision, larger or multi-PR work, research and security work. Search existing issues first and reuse a match; do not create duplicates. Small self-contained changes such as typo fixes, small documentation updates, dependency bumps and small CI or test fixes can go straight to a pull request that explains what changed and why. Keep issue comments short and limited to decisions, blockers and results. The complete lifecycle and label taxonomy are documented in [docs/project-workflow.md](docs/project-workflow.md).

Do not include recordings, transcripts, Hugging Face tokens, downloaded model weights, generated application bundles, or other private data in issues or commits. Synthetic fixtures are preferred for tests.

## Development setup

Install the platform prerequisites and uv in [docs/development.md](docs/development.md), then run:

```sh
make setup
make setup-checks
make hooks
make check
```

The application runtime does not require Python. Contributor checks use Python
3.12 or newer. `make setup-checks` syncs locked Python tooling into `.venv`
and downloads Chromium; hooks provision their own tools.
All Python work follows the [uv and pyproject.toml policy](docs/development.md#python-tooling-policy).
Use `uv add --dev` / `uv remove --dev` for contributor dependencies and include
the resulting `pyproject.toml` and `uv.lock` changes together. For an independently managed
[pre-commit](https://pre-commit.com/) installation, install
[uv](https://docs.astral.sh/uv/getting-started/installation/) first, then run:

```sh
uv tool install pre-commit==4.7.0
pre-commit --version
make hooks
```

The pre-commit hook scans staged changes with Gitleaks, rejects private keys,
checks workflow syntax/security, and runs Rust/Python/frontend/shell formatting,
ESLint, Ruff, ShellCheck, frontend diagnostics and Node tests. The pre-push hook
runs lightweight file hygiene and version consistency, without holding an open
SSH connection through compilation. Run the complete suite explicitly with
`make check` before pushing; CI also runs Python/browser regressions, all Rust
target tests, doctests and strict Clippy/rustdoc. Run pre-commit checks manually
with `make precommit` and the quick pre-push stage with `make prepush`. See the
[hook setup and untracked-file caveat](docs/ci-and-releases.md#local-setup-and-hook-scope).

## Testing changes

Use the narrowest relevant test while developing, then run `make check` before submitting:

| Command                             | Purpose                                                                             |
| ----------------------------------- | ----------------------------------------------------------------------------------- |
| `make check-fast`                   | All source formatting, portable lints, Svelte/TypeScript diagnostics and Node tests |
| `make fmt` / `make fmt-check`       | Apply / verify Rust, Python, frontend and shell formatting                          |
| `make check-portable`               | Frontend/Node/Python/shell format, lint, type, test and build checks                |
| `make test-python`                  | All deterministic Python tooling test modules                                       |
| `make python-lock-check`            | Verify Python lockfiles and generated benchmark requirements                        |
| `make audit-python`                 | Audit the locked contributor dependencies                                           |
| `make test-browser`                 | Existing mocked-native UI scenarios in Chromium                                     |
| `make workflows`                    | Actionlint/ShellCheck and zizmor workflow validation                                |
| `make test` / `make test-rust`      | All Rust workspace targets, doctests and the isolated probe                         |
| `make lint`                         | ESLint, Ruff, ShellCheck and strict Clippy                                          |
| `make check-rust` / `make doc-rust` | Compile all Rust targets / verify rustdoc with warnings denied                      |
| `make check`                        | Complete local and CI validation                                                    |
| `make version-check`                | Verify Cargo, npm, lockfile, and Tauri versions agree                               |
| `make build-debug`                  | Debug desktop application and platform bundle                                       |
| `make diagrams`                     | Render `docs/diagrams/*.mmd` to SVGs styled with the app's design tokens            |

Tests must not open a microphone, download model weights, access user application data, or depend on a network service. Use temporary directories and small synthetic fixtures. Model-backed and microphone checks belong in the explicit manual validation flow described in [docs/validation.md](docs/validation.md).

For behavior changes, add a regression test that fails without the change. For UI changes, attach synthetic screenshots to the PR when useful instead of committing them. Keep local captures in the ignored screenshot directories, and test both the browser preview and native shell when native behavior is involved.

## Pull requests

Local commits follow the owner's [small-commit convention](docs/project-workflow.md#local-commit-convention):
3–4 related implementation files, with their tests/docs when needed, and an issue
reference in each message when the work has an issue. Preserve the configured Git identity without adding
co-author/AI attribution. Local commit permission does not authorize a push.

Open every pull request from the [template](.github/PULL_REQUEST_TEMPLATE.md), link the tracking issue if there is one, and give it work-type labels (the issue's, when there is one). Tracked issues have a project item whose Status and Area you keep current as work moves (details in [project workflow](docs/project-workflow.md#keeping-status-and-area-current)). Commits are small, GPG-signed, reference the issue when there is one, and carry no co-author or AI attribution lines, in the commit messages or in PR and issue text. Do not bypass the installed hooks. Do not title a PR `Release vX.Y.Z`: that subject on `main` makes the release bot tag a release. Settings, secrets, rulesets, signing keys, tags and releases need the owner's explicit approval first.

Keep pull requests focused. Explain the user-visible effect and validation performed. Every completed job must update the relevant canonical documentation under `docs/`; update configuration examples and model provenance when they change. Preserve `Cargo.lock` and `package-lock.json` so CI uses the same dependency resolution.

CI must pass on the supported Node version, the macOS Rust tests and application bundle build, browser regressions, Python tooling tests, secret scanning, and npm/Python/RustSec dependency audits. Required status-check names, current GitHub plan limitations, version preparation, and release behavior are documented in [CI and releases](docs/ci-and-releases.md). A passing automated suite does not replace manual checks for microphone permissions, native model inference, cancellation, export behavior, or application packaging.

Follow the [community conduct expectations](CODE_OF_CONDUCT.md) and use
[SUPPORT.md](SUPPORT.md) to route questions. Application code is licensed under the
[Apache License 2.0](LICENSE). Model, font, runtime and dependency licenses remain
separate; see [licenses and model terms](docs/licenses-and-model-terms.md).
