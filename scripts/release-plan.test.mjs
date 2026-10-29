import assert from 'node:assert/strict';
import test from 'node:test';
import { nextVersion, stampChangelog } from './release-plan.mjs';

test('stable bumps', () => {
  assert.equal(nextVersion('0.1.0', 'patch'), '0.1.1');
  assert.equal(nextVersion('0.1.9', 'minor'), '0.2.0');
  assert.equal(nextVersion('0.9.4', 'major'), '1.0.0');
});

test('prerelease starts and increments candidates', () => {
  assert.equal(nextVersion('0.1.0', 'prerelease'), '0.1.1-rc.1');
  assert.equal(nextVersion('0.1.1-rc.1', 'prerelease'), '0.1.1-rc.2');
  assert.equal(nextVersion('0.1.1-rc.9', 'prerelease'), '0.1.1-rc.10');
});

test('release promotes a candidate and refuses a stable version', () => {
  assert.equal(nextVersion('0.1.1-rc.2', 'release'), '0.1.1');
  assert.throws(() => nextVersion('0.1.0', 'release'), /already a stable/);
});

test('stable bumps refuse to leave a pending candidate', () => {
  assert.throws(() => nextVersion('0.1.1-rc.1', 'patch'), /release candidate/);
});

test('rejects unknown bumps and unsupported version forms', () => {
  assert.throws(() => nextVersion('0.1.0', 'huge'), /Bump must be/);
  assert.throws(() => nextVersion('0.1.0-beta.1', 'patch'), /X\.Y\.Z/);
  assert.throws(() => nextVersion('0.1', 'patch'), /X\.Y\.Z/);
});

const CHANGELOG = `# Changelog

## [Unreleased]

### Fixed

- A fix.

## [1.2.0] - 2026-01-02

### Added

- A feature.

[Unreleased]: https://example.com/r/compare/v1.2.0...HEAD
[1.2.0]: https://example.com/r/releases/tag/v1.2.0
`;

test('stamp dates the Unreleased notes and moves the links', () => {
  assert.equal(
    stampChangelog(CHANGELOG, '1.2.1', '2026-03-04'),
    `# Changelog

## [Unreleased]

## [1.2.1] - 2026-03-04

### Fixed

- A fix.

## [1.2.0] - 2026-01-02

### Added

- A feature.

[Unreleased]: https://example.com/r/compare/v1.2.1...HEAD
[1.2.1]: https://example.com/r/releases/tag/v1.2.1
[1.2.0]: https://example.com/r/releases/tag/v1.2.0
`,
  );
});

test('stamp refuses empty notes, candidates and a version that exists', () => {
  const empty = CHANGELOG.replace('### Fixed\n\n- A fix.\n\n', '');
  assert.throws(() => stampChangelog(empty, '1.2.1', '2026-03-04'), /empty/);
  assert.throws(() => stampChangelog(CHANGELOG, '1.2.1-rc.1', '2026-03-04'), /candidate/);
  assert.throws(() => stampChangelog(CHANGELOG, '1.2.0', '2026-03-04'), /already/);
  assert.throws(() => stampChangelog(CHANGELOG, '1.2.1', '4 March'), /YYYY-MM-DD/);
});

test("stamp accepts the repository's own changelog", async () => {
  const { readFile } = await import('node:fs/promises');
  const text = await readFile(new URL('../CHANGELOG.md', import.meta.url), 'utf8');
  // [Unreleased] is empty right after a release, so add an entry first.
  const pending = text.replace('## [Unreleased]\n', '## [Unreleased]\n\n### Fixed\n\n- Probe.\n');
  const stamped = stampChangelog(pending, '99.0.0', '2099-01-01');
  assert.match(
    stamped,
    /^## \[Unreleased\]\n\n## \[99\.0\.0\] - 2099-01-01\n\n### Fixed\n\n- Probe\./m,
  );
  assert.match(stamped, /^\[99\.0\.0\]: \S+\/releases\/tag\/v99\.0\.0$/m);
});
