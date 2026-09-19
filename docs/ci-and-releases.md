# macOS CI, versions, and releases

The current desktop target is **macOS on Apple silicon**. Tauri bundles the
frontend inside the Mac application. Nginx and Docker belong only to an optional
manual browser preview and are not required to develop, test or release the app.
Ubuntu runners still execute portable frontend/tooling/security checks; that
runner choice does not make Linux a supported desktop product.

## Pull-request checks

Pull requests, merge-queue entries, main pushes, and manual runs execute checks
without accessing installed models, microphones, or user data. Version tags reuse
the same checks before packaging. Stable PR check names are:

1. `Check / Frontend and repository hygiene`
2. `Check / Browser regressions`
3. `Check / Rust (macOS)`
4. `Security / Secrets`
5. `Security / Dependencies`
6. `Security / Python benchmarks (audio-frontend)`
7. `Security / Python benchmarks (vibevoice)`

The frontend job runs repository hooks, actionlint with ShellCheck, zizmor,
and `make check-portable`: source formatting, ESLint, Ruff, shell linting,
synchronized-version checks, Svelte/TypeScript, all Node tooling tests, all
Python test modules, and the production frontend build. Python test discovery
handles `test-*.py`, `test_*.py`, and `*.test.py` in `scripts/`; every module runs
in an isolated interpreter, and any failure fails the job. These tests use
synthetic data and use the `dev` group in `pyproject.toml`, resolved by the
versioned `uv.lock`. Optional model/inference projects are kept separate.
`make python-lock-check` checks all Python lockfiles and generated benchmark
exports. CI installs uv 0.12.15 through a SHA-pinned action and selects Python
3.12. The frontend job caches downloads under `.cache/uv`, keyed by lockfiles;
only pushes to `main` save that cache. Security jobs disable cache reuse.

Native-protocol tests in `scripts/test-vibevoice-bitnet.py` launch synthetic
children through the real resource wrapper. It uses BSD `/usr/bin/time -l` on
macOS and GNU `/usr/bin/time -f` on the Ubuntu portable runner. GNU peak RSS is
reported in KiB and converted to bytes; macOS already reports bytes. The GNU
format is an explicit marker, so parsing does not depend on localized labels or
the caller's `TIME` environment variable. Missing memory evidence stays absent.
See GNU time's [format options](https://www.gnu.org/software/time/manual/html_node/Setting-Format.html)
and [memory fields](https://www.gnu.org/software/time/manual/html_node/Memory-Resources.html).
Using `-l` on Ubuntu exits before the child emits its protocol record and causes
misleading missing-measurement failures. Do not skip those assertions or move the
portable suite to macOS to conceal the failure. The full benchmark's macOS power
sampling remains platform-specific; this wrapper portability does not qualify
Linux inference or change the supported desktop platform.

The portability correction was validated locally on macOS with the complete
`make check-portable` gate, including nine passing protocol/resource tests and
all nine Python suites. Platform-selection and byte-conversion fixtures cover
both BSD and GNU output; the real subprocess test checks memory evidence on the
current host. This is local evidence, not a successful Ubuntu Actions rerun.

The browser job runs the existing `tests/recovery.html` scenarios in Chromium
with mocked Tauri commands through Playwright. It fails on scenario failures,
uncaught browser errors, or missing completion; it does not retry away failures.
On failure it retains only the synthetic test report, screenshot and trace for
seven days. Local commands are `make test-browser` and `make test-python`.

The native job uses the Apple-silicon `macos-15` runner with Rust 1.98.1 from
`rust-toolchain.toml`. It verifies the pinned patched runtime, runs native
completion-report tests, formatting, locked all-target compilation/tests,
separate doctests, strict Clippy and rustdoc with warnings denied,
then builds a debug `.app` and verifies its executable exists. Compiler upgrades
must pass these checks. Model/hardware tests remain explicit opt-in checks.
Linux, Windows and Intel Mac packaging/qualification are outside the current CI
scope. Runner architectures follow
[GitHub's runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners);
app packaging follows [Tauri's macOS bundle documentation](https://v2.tauri.app/distribute/macos-application-bundle/).

### Language coverage

| Source | Formatting | Lint/type/build checks | Tests |
| --- | --- | --- | --- |
| Rust workspace | `cargo fmt --all -- --check` | Locked `cargo check` and Clippy across all targets; Clippy/rustdoc deny warnings; native app build | `cargo test --workspace --all-targets --locked` plus a separate `--doc` run |
| Isolated Rust integrity probe | Its own manifest's rustfmt check | Locked all-target/all-feature check and strict Clippy | Its own all-target/all-feature test harness |
| TypeScript/Svelte | Prettier with the Svelte plugin | ESLint recommended JS/TS/Svelte rules, zero warnings; strict `svelte-check`, including the interaction preview fixture; Vite build | Node helper tests and mocked-native Playwright regressions |
| JavaScript/Node tools | Prettier | ESLint recommended rules, zero warnings | All existing `scripts/*.test.mjs` and frontend helper suites |
| Python tooling | Ruff format | Ruff syntax, Pyflakes, style-error and import-order rules | Every existing Python test naming convention in `scripts/` |
| POSIX shell scripts | shfmt syntax/format check | ShellCheck, including sourced project files | Exercised by native reporting/build/release checks where applicable |
| JSON/YAML/TOML/Actions | Root JSON uses Prettier; generic whitespace hooks | Syntax hooks, actionlint/ShellCheck and zizmor | Workflow checks and version/publication tooling tests |

All targets includes Rust example test harnesses that plain `cargo test` only
compiled. Cargo's `--all-targets` excludes doctests, so the separate `--doc`
command must remain. The isolated integrity probe is not a workspace member and
is checked explicitly. Its `hardware` feature is supported on the Mac runner;
the app's CUDA/Vulkan/ROCm features require other toolchains and are not enabled
together through `--all-features`. The current supported Mac configuration and
all of its deterministic tests are covered. Existing ignored model/hardware
qualification cases stay opt-in; CI does not download weights or open a microphone.
Python inference/benchmark entrypoints are parsed/linted/formatted without
executing their downloads or importing optional model runtimes. Python static
type checking is not claimed; most tooling is unannotated.

Direct development versions are pinned in npm/Python manifests: ESLint 10.10.0,
typescript-eslint 8.70.0, eslint-plugin-svelte 3.23.0, Prettier 3.9.8 with
prettier-plugin-svelte 4.1.1, Ruff 0.16.8, ShellCheck 0.11.0 via shellcheck-py
0.11.0.1, and shfmt 3.14.1 via shfmt-py 4.2.0. Node 24 is used in CI; the lint
toolchain requires at least Node 22.13 in the supported 22.x line. ESLint permits
intentionally unused underscore-prefixed callback arguments; Svelte TypeScript
names are resolved by `svelte-check` instead of ESLint's JavaScript `no-undef`.
No baseline of ignored lint findings is used.

Configuration follows the official [Cargo test reference](https://doc.rust-lang.org/cargo/commands/cargo-test.html),
[Ruff configuration](https://docs.astral.sh/ruff/configuration/),
[typescript-eslint setup](https://typescript-eslint.io/getting-started/),
[Svelte ESLint guide](https://sveltejs.github.io/eslint-plugin-svelte/user-guide/),
and [Prettier configuration](https://prettier.io/docs/configuration).

Workflows grant read-only repository access to pull-request code, disable
persisted checkout credentials, bound job durations, and cancel obsolete check
runs. Every third-party action and remote hook is pinned to a full commit SHA.
The runtime cache is limited to test builds; release packaging does not restore build caches. Two narrow zizmor style annotations
retain `./` local workflow references until actionlint supports GitHub's newer
`$/` spelling; no vulnerability rule is disabled. The guidance follows
[GitHub secure-use practices](https://docs.github.com/en/actions/reference/security/secure-use),
[actionlint](https://github.com/rhysd/actionlint/blob/main/docs/usage.md), and
[zizmor](https://docs.zizmor.sh/integrations/).

GitHub makes required status checks enforceable through branch protection or
rulesets. Those controls are available for public repositories on GitHub Free,
but private repositories require GitHub Pro, Team, or Enterprise. This private
repository's rulesets API returned HTTP 403 during the 2026-09-19 review. Workflow
files can be prepared locally, but the current plan cannot enforce these rules. After changing
visibility or plan, protect `main`, require a pull request, require the five exact
checks above, require branches to be up to date, dismiss stale approvals, and
require conversation resolution. See GitHub's [ruleset availability](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets).

Do not mark release-only jobs as required PR checks. Job names above are an API
contract for branch protection and should change only with a coordinated ruleset
update.

## Local setup and hook scope

After installing the native prerequisites and running `make setup`, use Python
3.12 or newer for contributor checks (the native application still needs no
Python runtime):

```sh
make setup-checks
make hooks
make precommit
make workflows
make check
```

Install [uv](https://docs.astral.sh/uv/getting-started/installation/) first.
`setup-checks` runs `uv sync --locked` to create `.venv` from `pyproject.toml`
and `uv.lock`, then downloads Chromium. Python 3.12+ is required; uv can provision
the interpreter. Old `.tooling/ci-venv` directories are preserved and unused.
On Linux install Chromium system libraries
with `PLAYWRIGHT_BROWSERS_PATH="$PWD/.cache/playwright" npm exec playwright install --with-deps chromium`. Hooks provision their own
isolated environments, including Go-based actionlint, ShellCheck and Gitleaks;
the first installation requires network access. `make hooks` installs both
pre-commit and pre-push. `make precommit` runs the pre-commit stage; `make prepush`
runs lightweight file hygiene and version-consistency checks. The automatic push
hook does not compile Rust, run browser tests or invoke `make check`: Git has
already contacted the remote when it invokes pre-push, so a long build can leave
the SSH connection idle until it disconnects. Run `make check` explicitly before
`git push` for full local validation, without an open push connection. All full
checks remain in the existing PR/main/merge-group CI jobs. SSH keep-alives can
help an unreliable connection, but are not a replacement for this shorter hook.
Existing installed hooks read the updated configuration; reinstalling or using
`--no-verify` is unnecessary. Initial tool provisioning still needs setup and may
take longer than a run with cached tools.

`make check-fast` runs formatting for
Rust/Python/frontend/shell, ESLint/Ruff/ShellCheck, Svelte/TypeScript diagnostics
and Node tests. `make fmt` applies all source formatters; `make fmt-check` checks
without editing. `make lint` runs all language linters. Individual targets use
the `-rust`, `-web`, `-python` and `-shell` suffixes (for example,
`make lint-python fmt-python-check`). `make check` adds Python/browser/native
tests, all Rust target/doctest runs, compilation and documentation checks;
workflow/security scans are separate
commands rather than silently included in that target.

Hooks normally enforce a 1 MiB per-file limit, including already-tracked files.
Exactly three existing public benchmark evidence JSON files listed in
`.pre-commit-config.yaml` have a separately enforced 5 MiB limit. Patch whitespace
is preserved for both native-test and runtime patches. Recordings, model weights,
local database scratch files, release outputs and browser reports remain ignored.

`pre-commit run --all-files` uses Git's tracked-file list. It does **not** validate
an entirely untracked checkout. Before an initial baseline, pass the explicit
`git ls-files --cached --others --exclude-standard` file list to `pre-commit run
--files`; never stage files merely to make validation run. Gitleaks' staged hook
also cannot establish coverage of untracked files; scan an isolated copy of the
eligible source files with `gitleaks dir --redact` separately. Never point a broad
scanner or artifact upload at local private recordings or downloaded models.

Refresh hook revisions periodically with `uv run --locked pre-commit autoupdate --freeze`, review
the changes, and rerun checks. Dependabot manages Actions pins, not these hook
revisions. Python direct and transitive dependencies are locked by uv. Change
contributor dependencies with `uv add --dev` / `uv remove --dev` and include the
manifest and lockfile together. Benchmark projects retain generated requirements
exports for existing consumers; regenerate them using the commands below and
copy each export to its corresponding `benchmarks/asr/` requirements file:

```sh
mkdir -p .cache
uv export --locked --project benchmarks/asr/python/audio-frontend --no-dev --no-emit-project --output-file .cache/audio-frontend-requirements.txt
uv export --locked --project benchmarks/asr/python/vibevoice --no-dev --no-emit-project --output-file .cache/vibevoice-reference-requirements.txt
```

The export commands intentionally match `make python-lock-check`, including the
generated header. Keep hashes and environment markers; do not hand-edit exports.
Dependabot updates the uv manifests/locks; its benchmark PRs need regenerated
exports before the check passes. Historical benchmark results require fresh
qualification before attributing them to changed environments.

Frontend coverage is shared between local hooks and CI:

| Check | Local command | Hook stage |
| --- | --- | --- |
| TypeScript/Svelte/JavaScript formatting (Prettier) | `make fmt-web-check` | pre-commit |
| TypeScript/Svelte/JavaScript lint (ESLint, warnings denied) | `make lint-web` | pre-commit |
| Svelte and TypeScript diagnostics | `make check-web` | pre-commit |
| Frontend helper and repository tests (Node) | `make test-tools` | pre-commit |
| Version consistency | `make version-check` | pre-push |
| Mocked-native UI regressions (Playwright/Chromium) | `make test-browser` | Explicit local check and CI |

`make hooks` installs both stages. CI runs those checks independently of local
hook installation; the portable job also builds frontend assets.

Hook staging follows [pre-commit's stage configuration](https://pre-commit.com/#confining-hooks-to-run-at-certain-stages).
The remote-ref timing is documented in [Git's pre-push hook contract](https://git-scm.com/docs/githooks#_pre_push).

Local verification on 19 September 2026: the actual `make prepush` stage finished
in 0.50 seconds with existing hook environments. A deliberate Tauri version
mismatch failed in 0.47 seconds, and its file was restored afterward. Cargo,
npm, Chromium and CMake were replaced with failing sentinels on the test PATH;
none was invoked. The pre-commit stage, explicit `make check` recipes and hosted
CI jobs are unchanged. These are local warm-tool measurements, not a promise
about first-time setup or network latency.

## Security gates

Gitleaks 8.28.0 runs on pull requests, merge groups, default-branch/tag pushes,
release publication, the weekly schedule, and manual security runs. CI downloads
the pinned Linux executable, verifies its exact SHA-256, and runs the CLI over
all available Git history. This avoids the wrapper action's restricted event
list, which excludes release and merge-group events. When updating the binary,
update the version and checksum together from the official release. Trivy
0.74.0 scans the checked-out tree so a secret remains detectable even when an
unusual merge history hides its introducing commit. The local pre-commit hook
also runs Gitleaks against staged changes and the generic hooks reject private
keys. Never add a broad allowlist to make a finding disappear: verify a false
positive and suppress only its exact fingerprint with a documented reason. The current exact fingerprints cover one existing
runtime digest plus 45 authored `constructed-NNNN` episode IDs and 17 public
source/native-library SHA-256 values. They do not exclude entire files, paths or
rules; a synthetic API-key control was separately confirmed to fail the scanner.

The dependency job installs packages without lifecycle scripts, runs `npm audit`
against production **and development** dependencies, runs `make audit-python`,
and runs cargo-audit 0.22.2 against RustSec with
known vulnerabilities and `unsound` advisories denied. Maintenance warnings stay
visible for dependency-upgrade planning without broad ignores. Separate
Apple-silicon macOS matrix jobs audit both isolated Python benchmark graphs;
one failure does not cancel the other job. Dependabot checks
npm, Cargo, GitHub Actions, Docker and the three uv projects weekly, with
routing labels, five open PRs per ecosystem and a seven-day version-update
cooldown. Security-update behavior is governed separately by Dependabot. The
optional manually dispatched container job scans the
final runnable image and fails for high or critical vulnerabilities, including
findings that do not yet have an upstream fix. A temporary exception therefore
requires an explicit, narrow, reviewed policy change rather than being silently
filtered.

`make audit-python` generates an ignored, hashed requirements export of the
locked contributor groups (`dev` and `benchmark`), then runs
`uv run --locked pip-audit --strict --require-hashes
--disable-pip` against it. The audit cannot silently resolve newer transitive
versions. `make audit-python-audio-frontend` and `make audit-python-vibevoice`
use the same auditor against fresh exports of their owning locks. They require
Apple-silicon macOS: running these exports through a Linux auditor would skip
their platform-marked dependencies and could produce an empty, misleading pass.
The audit jobs install only contributor tools, not model runtimes or weights.
All manifests and lockfiles stay versioned, while `.venv` and `.cache` stay ignored.

### Dependency security status and upstream limits

The 2026-09-19 remediation resolves 12 of the 17 inventoried Dependabot findings
in the local dependency graph. These changes have to reach the default branch
before GitHub can re-evaluate its alerts. No alert was dismissed, dependency
marker removed, or advisory ignored. [Issue #57](https://github.com/rahulsharmavishwakarma/transcribe/issues/57)
tracks the remaining work; this is not a claim that the repository is free of
known advisories.

| Optional VibeVoice dependency | Result | Dependabot alerts addressed |
| --- | --- | --- |
| Accelerate 1.10.1 | Removed; the CPU probe does not use its APIs or device dispatch | 17 |
| Torch 2.8.0 | Updated to 2.13.0 | 11, 12, 15 |
| Diffusers 0.35.1 | Updated to 0.38.0, requiring Safetensors 0.8.0 | 9, 10 |
| Transformers 4.51.3 | Updated to 4.57.6, with Tokenizers 0.22.2 | 2–7 |

The remaining Transformers alerts are `GHSA-69w3-r845-3855`,
`GHSA-29pf-2h5f-8g72`, `GHSA-fgcw-684q-jj6r`, and
`GHSA-xrqw-3rrv-vx5w` (Dependabot 8, 13, 14, 16). Their listed fixes require
Transformers 5, up to 5.10.0. The pinned Microsoft VibeVoice source explicitly
requires Transformers `<5.0.0`. It imports the removed Qwen2 fast-tokenizer module
and uses the older `tie_weights` signature and tied-weight key representation.
A version-only upgrade is incompatible; a maintained source port or a compatible
upstream release needs its own implementation and model qualification.
[Microsoft dependency contract](https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/pyproject.toml),
[pinned tokenizer](https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/vibevoice/modular/modular_vibevoice_text_tokenizer.py),
[Transformers 5.10 Qwen2 modules](https://github.com/huggingface/transformers/tree/v5.10.0/src/transformers/models/qwen2),
[model-loading API](https://github.com/huggingface/transformers/blob/v5.10.0/src/transformers/modeling_utils.py).

The local VibeVoice audit fails with eight returned records representing five
distinct PYSEC IDs: four corresponding to those Dependabot alerts and
`PYSEC-2025-217` / `CVE-2025-14929` for X-CLIP checkpoint conversion. Three IDs
appear twice in the advisory response; do not count those as separate flaws or
discard the extra X-CLIP finding. The latter also affects 4.57.6 according to the
[PyPA advisory](https://github.com/pypa/advisory-database/blob/main/vulns/transformers/PYSEC-2025-217.yaml).
These are optional research dependencies, outside the packaged desktop runtime.
Verified pinned source/artifacts, offline loading and the probe's limited APIs
reduce exposure; they do not remediate the package-level findings.

Dependabot alert 1 is `GHSA-wrw7-89jp-8q8g` / `RUSTSEC-2024-0429` in GLib 0.18.5.
Tauri 2.11.5's Linux GTK3/WebKitGTK dependency graph requires the 0.18 family;
the advisory's patched range starts at 0.20. A direct dependency on 0.20 would
leave the affected transitive copy present. The upstream 0.18 branch still
contains the affected iterator implementation. Fixing that graph needs a
compatible upstream backport or a separately reviewed maintained patch/fork;
manually changing the lockfile version is not a fix.
[RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2024-0429.html),
[Tauri dependencies](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.5/crates/tauri/Cargo.toml),
[GLib 0.18 implementation](https://github.com/gtk-rs/gtk-rs-core/blob/0.18/glib/src/variant_iter.rs).

`cargo tree --workspace --locked --target aarch64-apple-darwin --invert glib`
and the Intel macOS equivalent have no GLib dependency; the Linux equivalent
does. This establishes target exclusion for the Mac build, not a repaired
cross-platform lockfile. The new `cargo audit --deny unsound` fails on this
advisory even though cargo-audit classifies it as a soundness warning. Six other
maintenance warnings remain visible: `RUSTSEC-2024-0370` and the `unic-*`
advisories `RUSTSEC-2025-0075`, `0080`, `0081`, `0098`, and `0100`.

Local validation on macOS passed lock/export checks, audits of all contributor
groups and the audio frontend, all pre-commit checks (including workflow security,
formatting, language lints/types and Node tests), and 51 tests across ten Python
suites. The updated VibeVoice import/load/inference smoke passed without
Accelerate. The smoke used verified existing artifacts and
one 4.62-second public FLEURS recording; both chunks completed with full audio
coverage and no case error. This verifies basic compatibility, not repeatability
of dated accuracy/latency results or complete model qualification. The VibeVoice
and strict Rust audits intentionally remain failing until their upstream
blockers are resolved; CI has no `continue-on-error` or advisory exceptions.

CodeQL and private-repository dependency review are not enabled because this
repository does not currently have GitHub Advanced Security. Revisit those gates
if the repository becomes public or the plan changes. Regardless of scanner
output, a committed real secret must be revoked and rotated; deleting the text in
a later commit is not remediation.

## Optional browser-preview container

The default development preview and browser tests use Vite directly. The existing
root `Dockerfile` and `.github/container/nginx.conf` provide a separate static
preview for contributors who explicitly want to run a container. Nginx serves
Vite assets on port 8080 with a health endpoint and response headers. It is not
bundled with the Mac app and cannot provide native recording or inference.

The container workflow runs **only through `workflow_dispatch`**. It builds and
scans a local image, never publishes to a registry, and is not a required PR,
merge-queue, main-branch or release check. Optional local use:

```sh
make container-smoke
docker run --rm -p 8080:8080 transcribe-browser-preview:local
```

The container's pinned dependencies remain in Dependabot because the optional
files still exist. There is no automatic GHCR publication, container attestation
or multi-architecture image release in the Mac application workflow.

## Version policy

Application releases use SemVer tags such as `v0.2.0` or `v0.2.0-rc.1`. Runtime,
model, settings-schema, and export-schema versions are separate compatibility
identifiers and do not follow the application version automatically.

The application version must agree in the Cargo workspace, every local package in
`Cargo.lock`, root and desktop npm manifests, `package-lock.json`, and Tauri
configuration. Check or update all declarations with:

```sh
make version-check
make version-set VERSION=0.2.0
make version-check TAG=v0.2.0
```

`make version-set` changes files but never creates a commit, tag, or release.
Review its diff and run `make check` in the version pull request.

## Release workflow and download URL

A pushed `vX.Y.Z` or `vX.Y.Z-prerelease` tag runs the reusable test/security gates,
verifies the synchronized application version, and builds the optimized macOS
app plus DMG using Tauri's existing bundler. Publishing a release in GitHub's UI
also triggers this workflow for its tag. The collected assets are:

- `transcribe-macos-arm64.dmg`, containing `transcribe.app` and an Applications link;
- `SHA256SUMS`, covering that DMG.

The version is recorded in the release tag and app bundle. The asset name stays
constant so a website download button can link to:

```text
https://github.com/rahulsharmavishwakarma/transcribe/releases/latest/download/transcribe-macos-arm64.dmg
```

A specific release has a permanent version URL, for example:

```text
https://github.com/rahulsharmavishwakarma/transcribe/releases/download/v0.1.0/transcribe-macos-arm64.dmg
```

These are planned endpoints until the workflow is pushed and a release succeeds.
The repository must be public for anonymous downloads. A prerelease does not
replace the latest stable link; use its version-specific URL. See GitHub's
[release-link documentation](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases).

`scripts/package-release.sh` checks the bundled version/executable, verifies the
exact versioned DMG with `hdiutil`, then copies it to the stable filename.
`scripts/publish-release.mjs` checks the local checksum before contacting GitHub.
For a new or draft release it uploads all assets while the release is still a
draft, then publishes it. Stable releases are marked latest; prereleases are
explicitly excluded from latest. Publish stable tags in version order so an old
newly published tag does not replace the website's latest download.

For an already-published release, the script only adds missing assets. It never
clobbers existing public downloads. A partial-upload retry requires any existing
asset's server-provided SHA-256 digest to match the corresponding local file;
otherwise it fails and requires a new version. A complete release is a no-op on
rerun. An upload failure leaves a newly created release as a draft. GitHub's UI
can publish an empty release before the build finishes; its download becomes
available only after the workflow successfully attaches the DMG.

Actions build artifacts expire after 14 days, but the DMG is attached separately
as a GitHub Release asset for the public download link. The publish job records
both URLs in the workflow summary. No temporary Actions artifact URL is used by
the website. No container or Linux desktop package is published.

No Developer ID signing or notarization is configured. DMG checksums establish
byte integrity, not publisher identity; the installer is not yet a signed and
notarized public distribution. Signing requires a separately approved setup and
Apple credentials. Intel Mac packaging is also outside the current target.
Tauri's [DMG guidance](https://v2.tauri.app/distribute/dmg/) documents the standard
app/Applications layout and the limits on customized icon positioning in CI.

Release sequence:

1. Run `make version-set VERSION=X.Y.Z` on a dedicated branch.
2. Update release notes and canonical documentation.
3. Run `make check`; on a Mac, `CI=true make build` builds the optimized app and DMG.
4. Inspect/install-test the local DMG before approving the source version.
5. Merge the reviewed changes and push the matching `vX.Y.Z` tag, or publish that
   version through GitHub Releases. These are maintainer actions, not actions
   performed by `make version-set` or by this setup task.
6. Wait for the Release workflow to pass and verify the attached DMG/checksum.
7. Use the stable latest link on the launch site, or a version URL for a pinned download.

Release workflows never download or bundle ASR/cleanup model weights. Model and
runtime downloads retain their own pinned versions, licenses, sizes and hashes.
`make build-debug` explicitly builds only the `.app`, keeping DMG creation out of
routine debug/native PR checks.

## Public-source exclusions

`.gitignore` excludes local screenshots in `screenshots/`, `docs/screenshots/`,
raster captures under `docs/validation/`, exploratory studies under `docs/design/`,
and generated `artifacts/` output. These files are preserved locally. Required
application icons/fonts, synthetic decoder fixtures, benchmark protocols/results,
model provenance, and canonical Markdown/JSON validation evidence remain eligible
for source control. Do not use a global `*.png` rule: the native application
needs its bundled icons. Screenshot references in public docs describe the
written validation without linking to ignored local files.

Ignored files already tracked by Git stay tracked; ignore rules do not rewrite
history or remove files from the index. The exclusions were established before
the initial project baseline, so local captures never need to enter that history.
Contributors may attach synthetic screenshots to PRs when useful, keeping those
captures outside the source tree's public history.

## Activation and open-source readiness

As of the 2026-09-19 review, GitHub lists no workflows. Configuring files,
installing local hooks and recording local commits does not activate hosted CI.
The owner has authorized small local commits with issue references; see the
[commit convention](project-workflow.md#local-commit-convention). A push still
requires separate authorization. No hosted run, optional container execution,
package publication or release is claimed by local validation. Linux builds are
outside the current desktop scope. Run each workflow
once and confirm the displayed job names before configuring required checks.

GitHub's workflow token defaults were verified as read-only, with PR approval
disabled. Keep those defaults, enable available dependency/security alerts, require
approval for external contributors' workflow runs, and enable private vulnerability
reporting when available. Code scanning/dependency review, mandatory branch
protection and private GitHub attestations depend on visibility and plan; workflow
files cannot enable an unavailable entitlement. Do not change visibility merely
to make a security feature available.

`CODEOWNERS`, contribution/support/conduct/security guidance and bug/feature/task
issue templates establish the contributor workflow. A CODEOWNERS file requests
review; it does not enforce it without branch rules. The owner has not yet selected
a project license. Do not advertise the repository as open-source licensed or
redistribute third-party model/runtime assets under a project license by assumption.
Choose a license explicitly and preserve each dependency's own license/notices.

## Local verification baseline

The final local run on macOS passed `make check`: 228 Rust tests, 84 Node tests,
42 Python tests across nine modules, the mocked-native Chromium regression page,
native reporting checks, strict Clippy, formatting, frontend diagnostics and the
production frontend build. Twenty-two opt-in Rust cases remained ignored. The
locked desktop executable also linked, and repeating `make setup-checks` succeeded.
The local interpreter was Python 3.14; hosted Python jobs select 3.12.

Actionlint/ShellCheck, zizmor, generic source hooks, documentation links, npm audit,
pip-audit and both Gitleaks/Trivy source-secret scans passed. Gitleaks' synthetic
secret control failed as expected. RustSec reported no known vulnerability and
retained the seven warnings described above. Generic hooks normalized whitespace
or final newlines in five existing source/documentation assets. The local Git
index remained empty; hosted runs and Docker execution were not performed.

The macOS debug `.app` build passed locally. Its executable is resolved from
`CFBundleExecutable` in `Contents/Info.plist` and verified as executable, avoiding
an assumption that the Cargo binary name matches the application display name.
Workflow lint/security checks and explicit platform/trigger checks validate the
macOS-only native jobs and manual-only container workflow. Release collection
rejects non-macOS hosts. Hosted execution and optional container runs remain
unverified.

DMG/source-hygiene verification: an optimized Apple-silicon DMG was built locally,
verified with `hdiutil`, mounted read-only to check the versioned executable and
Applications link, and ejected. The stable-name copy and SHA256SUMS pass checksum
verification. All 60 existing local visual files were preserved byte-for-byte;
ignored captures/design experiments are excluded while required assets/evidence
remain eligible. Public documentation links were checked against ignore rules.
The tooling suite passes 94 Node tests, including ten release-publication cases
using a mocked GitHub client; no real release or visibility change was performed.

The subsequent language-check expansion passed 261 Rust tests (33 additional
example tests now executed by `--all-targets`), 94 Node tests, all 42 Python tests
across nine modules, the browser regression page and native reporting checks.
Twenty-two model/hardware cases remain explicitly ignored; doctest and isolated
probe harnesses currently contain no runnable cases. Rust all-target compilation,
strict Clippy/rustdoc, frontend build, Svelte/TypeScript including the interaction
preview, ESLint, Ruff, ShellCheck and all four formatters pass. Workflow lint,
generic source hooks and npm/Python dependency audits also pass. Rust local HTTP
fixtures and Python process checks required an unrestricted local run because
the execution sandbox denies loopback binding and `ps`; those reruns passed.

Existing Python/frontend/shell source was normalized once for the new formatters.
Lint fixes remove unused imports, retain caught error causes, explicitly mark
reactive dependency reads, add list keys and complete synthetic preview types.
The preview stays synthetic and no model/hardware validation is implied by these
changes. Required assets and ignored local visuals remain preserved. Hosted CI
still requires the owner's later commit/push; no files were staged or committed.
