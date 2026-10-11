import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  'apps',
  'desktop',
  'src-tauri',
);
const load = (name) => JSON.parse(fs.readFileSync(path.join(root, name), 'utf8'));

test('the updater plugin config block exists so startup deserialization succeeds', () => {
  // Regression guard: without plugins.updater the app panics at startup with
  // "Error deserializing 'plugins.updater' ... invalid type: null".
  const updater = load('tauri.conf.json').plugins?.updater;
  assert.ok(updater, 'tauri.conf.json must define plugins.updater');
  assert.equal(typeof updater.pubkey, 'string', 'pubkey is mandatory in updater 2.x config');
  assert.ok(Array.isArray(updater.endpoints), 'endpoints must be an array');
  for (const endpoint of updater.endpoints) {
    assert.match(endpoint, /^https:\/\//, 'updater endpoints must use HTTPS');
  }
});

test('updater authority stays on the main window and artifacts stay enabled', () => {
  const capability = load('capabilities/updater.json');
  assert.deepEqual(capability.windows, ['main']);
  assert.ok(capability.permissions.includes('updater:default'));
  assert.ok(capability.permissions.includes('process:default'));
  const macos = load('tauri.macos.conf.json');
  assert.equal(macos.bundle.createUpdaterArtifacts, true);
});
