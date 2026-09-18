import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { checkVersions, setVersion } from './version.mjs';

const projectRoot = path.resolve(import.meta.dirname, '..');

test('repository application versions are synchronized', () => {
  assert.equal(checkVersions(projectRoot, 'v0.1.0'), '0.1.0');
  assert.throws(() => checkVersions(projectRoot, 'v0.1.1'), /does not match/);
});

test('version setter updates every release declaration in an isolated copy', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'transcribe-version-'));
  for (const relative of [
    'Cargo.toml',
    'Cargo.lock',
    'package.json',
    'package-lock.json',
    'apps/desktop/package.json',
    'apps/desktop/src-tauri/tauri.conf.json',
  ]) {
    const destination = path.join(root, relative);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(path.join(projectRoot, relative), destination);
  }
  setVersion(root, '1.2.3-rc.1');
  assert.equal(checkVersions(root, 'v1.2.3-rc.1'), '1.2.3-rc.1');
});
