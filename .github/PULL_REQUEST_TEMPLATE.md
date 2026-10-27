## Summary

<!-- What changed, and why? -->

Tracking issue: # <!-- if any; small self-contained changes need no issue -->

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
- [ ] If there is a tracking issue, it has accurate labels, scope and any required architectural approval.
- [ ] This pull request has work-type labels (the issue's, when there is one); the issue's project Status and Area are current and read back.
- [ ] Commits are small, GPG-signed, reference the issue when there is one, and carry no co-author or AI attribution lines (also none in this description); hooks were not bypassed.
- [ ] The title does not start with `Release v` unless this is the release bot's version PR.
- [ ] Validation boxes above are checked only for checks I ran; unverified items are listed.
- [ ] Settings, secrets, rulesets, signing keys, tags or releases were changed only with the owner's approval recorded in a tracking issue.
- [ ] I did not commit model weights, recordings, credentials, or private data.
- [ ] I reviewed Gitleaks and dependency-audit findings without broad suppressions.
- [ ] New behavior preserves local-only processing and explicit user consent.
- [ ] Release metadata changed only when this pull request intentionally prepares a version.
