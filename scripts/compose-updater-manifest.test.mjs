import assert from 'node:assert/strict';
import test from 'node:test';
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

test('empty signatures, non-HTTPS URLs, bad versions and bad dates fail', () => {
  assert.throws(() => composeManifest({ ...base, signature: '  \n' }), /signature/);
  assert.throws(() => composeManifest({ ...base, url: 'http://insecure/latest.json' }), /HTTPS/);
  assert.throws(() => composeManifest({ ...base, version: '0.2' }), /version/);
  assert.throws(() => composeManifest({ ...base, pubDate: 'not a date' }), /pub_date/);
});
