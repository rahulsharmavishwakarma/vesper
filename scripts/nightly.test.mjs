import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  applyNightly,
  nightlyVersion,
  NIGHTLY_FEED,
  NIGHTLY_PUBKEY,
  NIGHTLY_IDENTIFIER,
  NIGHTLY_PRODUCT_NAME,
} from './nightly.mjs';
import { checkVersions } from './version.mjs';

const projectRoot = path.resolve(import.meta.dirname, '..');

test('a nightly after a stable release targets the next patch', () => {
  assert.equal(nightlyVersion('0.1.0', '20261020'), '0.1.1-nightly.20261020');
  assert.equal(nightlyVersion('1.9.9', '20270101'), '1.9.10-nightly.20270101');
});

test('a nightly after a release candidate targets that release', () => {
  assert.equal(nightlyVersion('0.1.1-rc.2', '20261020'), '0.1.1-nightly.20261020');
});

test('rejects malformed dates and versions', () => {
  assert.throws(() => nightlyVersion('0.1.0', '2026-10-20'), /YYYYMMDD/);
  assert.throws(() => nightlyVersion('0.1.0', '00000000'), /YYYYMMDD/);
  assert.throws(() => nightlyVersion('0.1', '20261020'), /Cannot derive/);
});

test('applyNightly gives a separate identity and a nightly-only update feed', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vesper-nightly-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  // Mirror the files version.mjs reads and writes.
  const files = ['Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json'];
  const copy = (relative) => {
    const destination = path.join(root, relative);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(path.join(projectRoot, relative), destination);
  };
  for (const file of files) copy(file);
  copy('apps/desktop/package.json');
  copy('apps/desktop/src-tauri/tauri.conf.json');
  for (const entry of fs.readdirSync(path.join(projectRoot, 'crates'))) {
    const manifest = `crates/${entry}/Cargo.toml`;
    if (fs.existsSync(path.join(projectRoot, manifest))) copy(manifest);
  }
  copy('apps/desktop/src-tauri/Cargo.toml');

  applyNightly(root, '0.1.1-nightly.20261020');

  assert.equal(checkVersions(root), '0.1.1-nightly.20261020');
  const config = JSON.parse(
    fs.readFileSync(path.join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'),
  );
  assert.equal(config.productName, NIGHTLY_PRODUCT_NAME);
  assert.equal(config.identifier, NIGHTLY_IDENTIFIER);
  assert.notEqual(config.identifier, 'com.vesper.desktop');
  assert.deepEqual(config.plugins.updater.endpoints, [NIGHTLY_FEED]);
  // Nightly trusts only its own key, never the stable one.
  assert.equal(config.plugins.updater.pubkey, NIGHTLY_PUBKEY);
  const stable = JSON.parse(
    fs.readFileSync(path.join(projectRoot, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'),
  );
  assert.notEqual(NIGHTLY_PUBKEY, stable.plugins.updater.pubkey);
  assert.match(NIGHTLY_FEED, /^https:\/\/[^/]+\.github\.io\/vesper\/nightly\.json$/);
  // The stable feed must be unreachable from a nightly build.
  assert.ok(!JSON.stringify(config).includes('releases/latest/download'));
});

test('applyNightly refuses anything that is not a nightly version', () => {
  assert.throws(() => applyNightly(projectRoot, '0.1.1'), /not a nightly/);
  assert.throws(() => applyNightly(projectRoot, '0.1.1-rc.1'), /not a nightly/);
});
