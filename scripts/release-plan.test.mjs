import assert from 'node:assert/strict';
import test from 'node:test';
import { changelog, nextVersion } from './release-plan.mjs';

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

test('changelog drops version bumps and merge noise', () => {
  const notes = changelog(
    [
      'Fix cramped headers (#72)',
      'Set application version to 0.1.0 for stable release (Refs #62)',
      "Merge remote-tracking branch 'origin/main' into x",
      '',
      'Add updater feed (#65)',
    ],
    '0.1.1',
  );
  assert.equal(notes, '## Vesper 0.1.1\n\n- Fix cramped headers (#72)\n- Add updater feed (#65)\n');
});

test('changelog states when nothing changed', () => {
  assert.equal(changelog([''], '0.1.1'), '## Vesper 0.1.1\n\n- No changes.\n');
});
