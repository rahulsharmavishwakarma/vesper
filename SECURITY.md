# Security policy

## Supported versions

| Version                                                                                              | Supported |
| ---------------------------------------------------------------------------------------------------- | --------- |
| Latest stable release ([Releases](https://github.com/rahulsharmavishwakarma/vesper/releases/latest)) | Yes       |
| `main`                                                                                               | Yes       |
| Older releases, release candidates and nightly builds                                                | No        |

Fixes ship in the next release. Update to the latest stable version before reporting.

## Reporting a vulnerability

Report vulnerabilities privately through
[**Security → Report a vulnerability**](https://github.com/rahulsharmavishwakarma/vesper/security/advisories/new).
Do not open a public issue, pull request or discussion for an undisclosed vulnerability.

Include the affected version or commit, the impact, steps to reproduce and any
suggested fix. Do not attach real recordings, transcripts, access tokens or other
private data; a small synthetic reproduction is best.

The maintainer aims to acknowledge a report within seven days, keep you updated
while it is investigated, and coordinate disclosure once a fix or mitigation is
available. Reporters are credited in the advisory unless they prefer otherwise.

## Scope

In scope: the Vesper app and its update mechanism, model downloads and hash
verification, imported-media handling, local storage of recordings and
transcripts, the native bindings, and the release and CI pipeline in this
repository. Vulnerabilities in third-party models or runtimes belong upstream;
report them here as well when Vesper's use of them makes the issue exploitable.

## How the project checks for vulnerabilities

Every pull request runs secret scanning (Gitleaks), npm, Cargo (RustSec) and Python
(pip-audit) dependency audits, CodeQL, and workflow security linting. Dependabot
checks dependencies weekly. Any scanner exception is narrow, documented and
approved; see [CI and releases](docs/ci-and-releases.md#dependency-security-status-and-upstream-limits).
If a secret is ever committed, it is revoked and rotated immediately; deleting it in
a later commit is not enough, because Git history keeps it.
