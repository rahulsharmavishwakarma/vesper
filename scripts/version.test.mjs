import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { checkVersions, setVersion, workspaceVersionedPackages } from './version.mjs';

const projectRoot = path.resolve(import.meta.dirname, '..');

// Independent oracle: every version-inheriting workspace member, hard-coded so
// the test does not reuse the production collector.
const EXPECTED_PACKAGES = [
  'vesper-audio',
  'vesper-core',
  'vesper-data',
  'vesper-desktop',
  'vesper-desktop-platform',
  'vesper-eval',
  'vesper-inference',
  'vesper-mlx',
  'vesper-models',
  'vesper-nemo',
];

function isolatedCopy(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vesper-version-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const relative of [
    'Cargo.toml',
    'Cargo.lock',
    'package.json',
    'package-lock.json',
    'apps/desktop/package.json',
    'apps/desktop/src-tauri/tauri.conf.json',
    'apps/desktop/src-tauri/Cargo.toml',
    'crates/vesper-audio/Cargo.toml',
    'crates/vesper-core/Cargo.toml',
    'crates/vesper-data/Cargo.toml',
    'crates/vesper-desktop-platform/Cargo.toml',
    'crates/vesper-eval/Cargo.toml',
    'crates/vesper-inference/Cargo.toml',
    'crates/vesper-mlx/Cargo.toml',
    'crates/vesper-models/Cargo.toml',
    'crates/vesper-nemo/Cargo.toml',
  ]) {
    const destination = path.join(root, relative);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(path.join(projectRoot, relative), destination);
  }
  return root;
}

// Independent oracle: reads Cargo.lock directly instead of collectVersions.
function lockVersions(root) {
  const text = fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8');
  const found = {};
  for (const match of text.matchAll(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/g)) {
    found[match[1]] = match[2];
  }
  return found;
}

test('repository application versions are synchronized', () => {
  assert.equal(checkVersions(projectRoot, 'v0.1.0-rc.2'), '0.1.0-rc.2');
  assert.throws(() => checkVersions(projectRoot, 'v0.1.1'), /does not match/);
});

test('workspace discovery covers every version-inheriting member', () => {
  assert.deepEqual(
    [...workspaceVersionedPackages(projectRoot)].sort(),
    [...EXPECTED_PACKAGES].sort(),
  );
});

test('version setter updates every release declaration in an isolated copy', (t) => {
  const root = isolatedCopy(t);
  setVersion(root, '1.2.3-rc.1');
  assert.equal(checkVersions(root, 'v1.2.3-rc.1'), '1.2.3-rc.1');
  const versions = lockVersions(root);
  for (const name of EXPECTED_PACKAGES) {
    assert.equal(versions[name], '1.2.3-rc.1', `Cargo.lock entry for ${name}`);
  }
});

test('numeric prerelease identifiers reject leading zeroes', (t) => {
  assert.throws(() => setVersion(isolatedCopy(t), '1.2.3-01'), /leading zeroes/);
  assert.throws(() => setVersion(isolatedCopy(t), '1.2.3-rc.01'), /leading zeroes/);
  assert.throws(() => checkVersions(projectRoot, 'v0.1.0-01'), /leading zeroes/);
  const root = isolatedCopy(t);
  setVersion(root, '1.2.3-0');
  assert.equal(checkVersions(root, 'v1.2.3-0'), '1.2.3-0');
});
