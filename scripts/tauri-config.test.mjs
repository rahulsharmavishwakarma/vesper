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

test('the updater key is enrolled with the stable feed only', () => {
  // Enrollment guard: an empty pubkey or endpoint list silently disables
  // update discovery. Only the stable feed is enrolled (Refs #65).
  const updater = load('tauri.conf.json').plugins?.updater;
  assert.ok(updater.pubkey.length > 0, 'updater pubkey must be enrolled');
  const decoded = Buffer.from(updater.pubkey, 'base64').toString('utf8');
  assert.match(
    decoded,
    /^untrusted comment: minisign public key: [0-9A-F]+\n/,
    'pubkey must be minisign format',
  );
  assert.deepEqual(updater.endpoints, [
    'https://github.com/rahulsharmavishwakarma/vesper/releases/latest/download/latest.json',
  ]);
});

test('updater authority stays on the main window and artifacts stay enabled', () => {
  const capability = load('capabilities/updater.json');
  assert.deepEqual(capability.windows, ['main']);
  assert.ok(capability.permissions.includes('updater:default'));
  assert.ok(capability.permissions.includes('process:default'));
  const macos = load('tauri.macos.conf.json');
  assert.equal(macos.bundle.createUpdaterArtifacts, true);
});

test('the bundled speaker runtime lands where the first-run installer looks', () => {
  // The installer reads Resources/diarization-runtime/<revision>; mapping the
  // parent staging directory nested the revision one level too deep.
  const resources = load('tauri.macos.conf.json').bundle.resources;
  const entries = Object.entries(resources).filter(([, to]) => to === 'diarization-runtime/');
  assert.deepEqual(entries, [
    ['../../../.tooling/diarization-runtime/nemotron-diarization-runtime/', 'diarization-runtime/'],
  ]);
  const rust = fs.readFileSync(
    path.join(root, '..', '..', '..', 'crates/vesper-models/src/diarization.rs'),
    'utf8',
  );
  assert.match(rust, /join\("\.\.\/Resources\/diarization-runtime"\)/);
  assert.match(rust, /bundled\.join\(RUNTIME_REVISION\)/);
});
