import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { ASSET_NAMES, publishRelease } from './publish-release.mjs';

function fixture(t, release = null, fail = () => false) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vesper-publish-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const content = 'Synthetic installer bytes; not an executable image';
  const digest = createHash('sha256').update(content).digest('hex');
  fs.writeFileSync(path.join(directory, ASSET_NAMES[0]), content);
  fs.writeFileSync(path.join(directory, ASSET_NAMES[1]), `${digest}  ${ASSET_NAMES[0]}\n`);
  const calls = [];
  const run = (args) => {
    calls.push(args);
    if (fail(args)) return { status: 1, stdout: '{"status":503}', stderr: 'Synthetic failure' };
    if (args[0] === 'api') {
      return release
        ? { status: 0, stdout: JSON.stringify(release) }
        : { status: 1, stdout: '{"status":404}' };
    }
    return { status: 0, stdout: '' };
  };
  return {
    directory,
    digest,
    calls,
    publish: (tag = 'v1.2.3', options = {}) =>
      publishRelease({ repository: 'example/vesper', tag, directory, run, ...options }),
  };
}

test('a new stable release stays draft until the DMG and checksums are uploaded', (t) => {
  const f = fixture(t);
  assert.equal(f.publish(), 'published');
  assert.deepEqual(
    f.calls.map((args) => args.slice(0, 2)),
    [
      ['api', 'repos/example/vesper/releases/tags/v1.2.3'],
      ['release', 'create'],
      ['release', 'upload'],
      ['release', 'edit'],
    ],
  );
  assert.ok(f.calls[1].includes('--draft'));
  assert.ok(f.calls[1].includes('--verify-tag'));
  assert.equal(f.calls[1][f.calls[1].indexOf('--title') + 1], 'Vesper v1.2.3');
  for (const name of ASSET_NAMES) assert.ok(f.calls[2].includes(path.join(f.directory, name)));
  assert.ok(f.calls[3].includes('--draft=false'));
  assert.ok(f.calls[3].includes('--latest=true'));
});

test('prerelease tags never replace the latest stable download', (t) => {
  const f = fixture(t);
  f.publish('v1.3.0-rc.1');
  assert.ok(f.calls[1].includes('--prerelease'));
  assert.ok(f.calls[1].includes('--latest=false'));
  assert.ok(f.calls.at(-1).includes('--prerelease=true'));
  assert.ok(f.calls.at(-1).includes('--latest=false'));
});

test('a failed upload cannot publish the draft', (t) => {
  const f = fixture(t, null, (args) => args[1] === 'upload');
  assert.throws(() => f.publish(), /Synthetic failure/);
  assert.equal(
    f.calls.some((args) => args[1] === 'edit'),
    false,
  );
});

test('rerunning a complete published release performs no mutations', (t) => {
  const f = fixture(t, { draft: false, assets: ASSET_NAMES.map((name) => ({ name })) });
  assert.equal(f.publish(), 'already-published');
  assert.equal(f.calls.length, 1);
});

test('a release published through GitHub gets its missing assets without clobbering', (t) => {
  const f = fixture(t, { draft: false, assets: [] });
  assert.equal(f.publish(), 'assets-attached');
  assert.equal(f.calls[1][1], 'upload');
  assert.equal(f.calls[1].includes('--clobber'), false);
  assert.equal(f.calls.length, 2);
});

test('a partial published upload can resume only when the existing DMG bytes match', (t) => {
  const release = { draft: false, assets: [{ name: ASSET_NAMES[0], digest: '' }] };
  const f = fixture(t, release);
  release.assets[0].digest = `sha256:${f.digest}`;
  assert.equal(f.publish(), 'assets-attached');
  assert.ok(f.calls[1].includes(path.join(f.directory, 'SHA256SUMS')));
  assert.equal(f.calls[1].includes(path.join(f.directory, ASSET_NAMES[0])), false);
});

test('a rebuilt DMG cannot replace published bytes or attach mismatched checksums', (t) => {
  const f = fixture(t, {
    draft: false,
    assets: [{ name: ASSET_NAMES[0], digest: 'sha256:different' }],
  });
  assert.throws(() => f.publish(), /use a new version tag/);
  assert.equal(f.calls.length, 1);
});

test('an existing prerelease draft retains that status even for a stable-looking tag', (t) => {
  const f = fixture(t, { draft: true, prerelease: true, assets: [] });
  assert.equal(f.publish(), 'published');
  assert.equal(
    f.calls.some((args) => args[1] === 'create'),
    false,
  );
  assert.ok(f.calls[1].includes('--clobber'));
  assert.ok(f.calls[2].includes('--latest=false'));
});

test('invalid tags and mismatched checksums fail before contacting GitHub', (t) => {
  const f = fixture(t);
  assert.throws(() => f.publish('v1.2.3;echo unsafe'), /Release tag/);
  fs.writeFileSync(path.join(f.directory, 'SHA256SUMS'), 'wrong digest\n');
  assert.throws(() => f.publish(), /does not match/);
  assert.equal(f.calls.length, 0);
});

test('draft-only mode stages a new candidate without publishing it', (t) => {
  const f = fixture(t);
  assert.equal(f.publish('v1.2.3', { publish: false }), 'draft-updated');
  assert.deepEqual(
    f.calls.map((args) => args.slice(0, 2)),
    [
      ['api', 'repos/example/vesper/releases/tags/v1.2.3'],
      ['release', 'create'],
      ['release', 'upload'],
    ],
  );
  assert.ok(f.calls[1].includes('--draft'));
  assert.ok(f.calls[2].includes('--clobber'));
});

test('draft-only mode refreshes an existing draft without publishing it', (t) => {
  const f = fixture(t, { draft: true, prerelease: false, assets: [] });
  assert.equal(f.publish('v1.2.3', { publish: false }), 'draft-updated');
  assert.ok(f.calls[1].includes('--clobber'));
  assert.equal(
    f.calls.some((args) => args[1] === 'edit'),
    false,
  );
});

test('draft-only mode never mutates an already-published release', (t) => {
  const f = fixture(t, { draft: false, assets: ASSET_NAMES.map((name) => ({ name })) });
  assert.equal(f.publish('v1.2.3', { publish: false }), 'already-published');
  assert.equal(f.calls.length, 1);
});

test('API failures are not mistaken for an absent release', (t) => {
  const f = fixture(t, null, (args) => args[0] === 'api');
  assert.throws(() => f.publish(), /Could not read release state/);
  assert.equal(f.calls.length, 1);
});
