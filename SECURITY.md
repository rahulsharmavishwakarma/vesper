# Security policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately through the repository's **Security → Report a vulnerability** page, when enabled. If it is unavailable, use a private contact method on the
[maintainer profile](https://github.com/rahulsharmavishwakarma) to request a secure
reporting channel. Do not open a public issue for an undisclosed vulnerability.

Include the affected commit or version, impact, reproduction steps, and any suggested mitigation. Do not attach real recordings, transcripts, access tokens, model credentials, or unrelated local data. A minimal synthetic reproduction is preferred.

Maintainers should acknowledge a report within seven days, keep the reporter informed while it is investigated, and coordinate disclosure after a fix or mitigation is available.

## Supported versions

Until Vesper publishes stable releases, security fixes target the latest revision of the default branch. Older snapshots are not maintained.

## Security-sensitive areas

Changes involving local file paths, model downloads and hashes, imported media, dataset exports, native bindings, application permissions, or persisted transcripts require tests for invalid and adversarial input. Network access, data upload, and automatic model activation must never be introduced silently.

## Automated checks

Pull requests scan staged/current content and Git history with Gitleaks and Trivy,
reject private keys, audit npm production/build tools, Cargo dependencies and Python test tooling, and lint workflow security. The optional manual container workflow separately
scans its browser-preview image for high/critical vulnerabilities. Dependabot
checks npm, Cargo, GitHub Actions, Docker and Python development inputs weekly.
Actionlint/ShellCheck and zizmor check workflow syntax and risky Actions patterns. See
[CI, containers, versions, and releases](docs/ci-and-releases.md) for exact status
checks and local hook setup.

A scanner suppression must be narrow, documented, and reviewed. If a real secret
is committed, revoke and rotate it immediately; removing it in a later commit is
not sufficient because Git history retains the value.
