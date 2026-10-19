import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { PLATFORM, composeManifest } from './compose-updater-manifest.mjs';

const base = {
  version: '0.2.0',
  notes: 'Synthetic release notes',
  pubDate: '2026-10-10T12:00:00Z',
  signature: 'dGVzdHNpZ25hdHVyZQ==\n',
  url: 'https://github.com/example/vesper/releases/download/v0.2.0/vesper-macos-arm64.app.tar.gz',
};

test('the manifest carries the literal signature and versioned archive URL', () => {
  const manifest = composeManifest(base);
  assert.equal(manifest.version, '0.2.0');
  assert.equal(manifest.pub_date, '2026-10-10T12:00:00.000Z');
  assert.deepEqual(Object.keys(manifest.platforms), [PLATFORM]);
  assert.equal(manifest.platforms[PLATFORM].signature, 'dGVzdHNpZ25hdHVyZQ==');
  assert.equal(manifest.platforms[PLATFORM].url, base.url);
});

test('prerelease versions compose for the separate beta feed', () => {
  const manifest = composeManifest({ ...base, version: '0.3.0-rc.1' });
  assert.equal(manifest.version, '0.3.0-rc.1');
});

test('the CLI accepts multi-line release notes', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vesper-manifest-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const archive = path.join(dir, 'app.tar.gz');
  const sig = path.join(dir, 'app.tar.gz.sig');
  const out = path.join(dir, 'latest.json');
  fs.writeFileSync(archive, 'archive');
  fs.writeFileSync(sig, 'dGVzdHNpZ25hdHVyZQ==\n');
  const notes = 'First line\n\n## Changes\n- item one\n- item two';
  const result = spawnSync(
    process.execPath,
    [
      fileURLToPath(new URL('./compose-updater-manifest.mjs', import.meta.url)),
      '--version=0.2.0',
      `--archive=${archive}`,
      `--sig=${sig}`,
      '--url=https://github.com/example/vesper/releases/download/v0.2.0/vesper-macos-arm64.app.tar.gz',
      `--notes=${notes}`,
      `--out=${out}`,
    ],
    { encoding: 'utf8' },
  );
  assert.equal(result.status, 0, result.stderr);
  const manifest = JSON.parse(fs.readFileSync(out, 'utf8'));
  assert.equal(manifest.notes, notes);
  assert.equal(manifest.version, '0.2.0');
});

test('empty signatures, non-HTTPS URLs, bad versions and bad dates fail', () => {
  assert.throws(() => composeManifest({ ...base, signature: '  \n' }), /signature/);
  assert.throws(() => composeManifest({ ...base, url: 'http://insecure/latest.json' }), /HTTPS/);
  assert.throws(() => composeManifest({ ...base, version: '0.2' }), /version/);
  assert.throws(() => composeManifest({ ...base, pubDate: 'not a date' }), /pub_date/);
});
