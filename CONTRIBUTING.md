# Contributing to transcribe

Thank you for improving transcribe. Contributions should keep speech processing local, preserve original user data, and make collection or destructive behavior explicit.

## Before you start

Every repository job requires a matching GitHub issue before work begins, including bug fixes, tests, documentation, research, and focused maintenance. Search existing issues and reuse the matching one; do not create duplicates. Apply the relevant work-type and routing labels, and keep scope, progress, decisions, documentation, validation, and remaining work updated through `gh`. The complete lifecycle and label taxonomy are documented in [docs/project-workflow.md](docs/project-workflow.md).

Do not include recordings, transcripts, Hugging Face tokens, downloaded model weights, generated application bundles, or other private data in issues or commits. Synthetic fixtures are preferred for tests.

## Development setup

Install the platform prerequisites in [docs/development.md](docs/development.md), then run:

```sh
make setup
make setup-checks
make hooks
make check
```

The application runtime does not require Python. Contributor checks use Python
3.12 or newer. `make setup-checks` installs pinned Python tooling into an isolated
project environment and downloads Chromium; hooks provision their own tools.
For an independently managed [pre-commit](https://pre-commit.com/) installation:

```sh
pipx install pre-commit==4.6.1
pre-commit --version
make hooks
```

The pre-commit hook scans staged changes with Gitleaks, rejects private keys,
checks workflow syntax/security, and runs Rust/Python/frontend/shell formatting,
ESLint, Ruff, ShellCheck, frontend diagnostics and Node tests. The pre-push hook
runs the complete suite, including Python/browser regressions, all Rust target
tests, doctests and strict Clippy/rustdoc. Run pre-commit checks manually
with `make precommit` and the pre-push stage with `make prepush`. See the
[hook setup and untracked-file caveat](docs/ci-and-releases.md#local-setup-and-hook-scope).

## Testing changes

Use the narrowest relevant test while developing, then run `make check` before submitting:

| Command | Purpose |
| --- | --- |
| `make check-fast` | All source formatting, portable lints, Svelte/TypeScript diagnostics and Node tests |
| `make fmt` / `make fmt-check` | Apply / verify Rust, Python, frontend and shell formatting |
| `make check-portable` | Frontend/Node/Python/shell format, lint, type, test and build checks |
| `make test-python` | All deterministic Python tooling test modules |
| `make test-browser` | Existing mocked-native UI scenarios in Chromium |
| `make workflows` | Actionlint/ShellCheck and zizmor workflow validation |
| `make test` / `make test-rust` | All Rust workspace targets, doctests and the isolated probe |
| `make lint` | ESLint, Ruff, ShellCheck and strict Clippy |
| `make check-rust` / `make doc-rust` | Compile all Rust targets / verify rustdoc with warnings denied |
| `make check` | Complete local and CI validation |
| `make version-check` | Verify Cargo, npm, lockfile, and Tauri versions agree |
| `make build-debug` | Debug desktop application and platform bundle |

Tests must not open a microphone, download model weights, access user application data, or depend on a network service. Use temporary directories and small synthetic fixtures. Model-backed and microphone checks belong in the explicit manual validation flow described in [docs/validation.md](docs/validation.md).

For behavior changes, add a regression test that fails without the change. For UI changes, attach synthetic screenshots to the PR when useful instead of committing them. Keep local captures in the ignored screenshot directories, and test both the browser preview and native shell when native behavior is involved.

## Pull requests

Local commits follow the owner's [small-commit convention](docs/project-workflow.md#local-commit-convention):
3–4 related implementation files, with their tests/docs when needed, and an issue
reference in every message. Preserve the configured Git identity without adding
co-author/AI attribution. Local commit permission does not authorize a push.

Keep pull requests focused and link the tracking issue. Explain the user-visible effect and validation performed. Every completed job must update the relevant canonical documentation under `docs/`; update configuration examples and model provenance when they change. Preserve `Cargo.lock` and `package-lock.json` so CI uses the same dependency resolution.

CI must pass on the supported Node version, the macOS Rust tests and application bundle build, browser regressions, Python tooling tests, secret scanning, and npm/Python/RustSec dependency audits. Required status-check names, current GitHub plan limitations, version preparation, and release behavior are documented in [CI and releases](docs/ci-and-releases.md). A passing automated suite does not replace manual checks for microphone permissions, native model inference, cancellation, export behavior, or application packaging.

Follow the [community conduct expectations](CODE_OF_CONDUCT.md) and use
[SUPPORT.md](SUPPORT.md) to route questions. The project owner has not yet chosen a
license; contributor files alone do not grant an open-source license. Model,
font, runtime and dependency licenses remain separate.
