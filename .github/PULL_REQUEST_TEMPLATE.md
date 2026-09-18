## Summary

<!-- What changed, and why? -->

Tracking issue: #

Canonical documentation updated: `docs/...`

## Validation

<!-- List the automated and manual checks you ran. -->

- [ ] `make check`
- [ ] `make version-check`
- [ ] `make precommit` (includes workflow syntax and security linting)
- [ ] I tested user-visible changes manually, or they do not require manual testing.
- [ ] I updated documentation and tests where needed.

## Screenshots or recordings

<!-- Include these for UI changes. Remove this section when it does not apply. -->

## Checklist

- [ ] I created and checked out a new feature branch before the first edit; the PR targets the default branch, and its commit list and diff contain only this task's scope.
- [ ] The branch uses the documented `<type>/<short-kebab-description>` convention and is not named after an agent or contributor.
- [ ] I preserved existing local work separately and did not leave duplicate implementation changes on `main`.
- [ ] This change is focused and does not include unrelated generated files.
- [ ] The tracking issue has accurate labels, scope, validation, and any required architectural approval.
- [ ] I did not commit model weights, recordings, credentials, or private data.
- [ ] I reviewed Gitleaks and dependency-audit findings without broad suppressions.
- [ ] New behavior preserves local-only processing and explicit user consent.
- [ ] Release metadata changed only when this pull request intentionally prepares a version.
