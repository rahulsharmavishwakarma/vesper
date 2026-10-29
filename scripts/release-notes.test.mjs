import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { releaseNotes, releaseNotesFromFile } from './release-notes.mjs';

const CHANGELOG = `# Changelog

Intro text.

## [Unreleased]

### Fixed

- Pending fix.

## [1.2.0] - 2026-10-01

### Added

- A feature that wraps
  onto a second line.

[Unreleased]: https://example.com/compare/v1.2.0...HEAD
[1.2.0]: https://example.com/releases/tag/v1.2.0
`;

test('a released version gets its own section without link references', () => {
  assert.equal(
    releaseNotes(CHANGELOG, 'v1.2.0'),
    '### Added\n\n- A feature that wraps\n  onto a second line.',
  );
});

test('a candidate uses its version section, or Unreleased before one exists', () => {
  assert.equal(releaseNotes(CHANGELOG, '1.2.0-rc.3'), releaseNotes(CHANGELOG, '1.2.0'));
  assert.equal(releaseNotes(CHANGELOG, 'v1.3.0-rc.1'), '### Fixed\n\n- Pending fix.');
});

test('empty notes fail instead of shipping a release without them', () => {
  assert.throws(() => releaseNotes('# Changelog\n\n## [Unreleased]\n\n', 'v2.0.0'), /no notes/);
});

test("the repository changelog has notes for the current version's release", () => {
  const { version } = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  assert.ok(releaseNotesFromFile(version).length > 0);
});
