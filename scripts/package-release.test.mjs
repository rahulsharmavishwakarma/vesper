import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fs.readFileSync(
  path.join(path.dirname(fileURLToPath(import.meta.url)), 'package-release.sh'),
  'utf8',
);

test('unsigned candidates skip updater artifacts without failing', () => {
  assert.match(script, /no updater archive; skipping updater artifacts/);
});

test('an updater archive without a signature fails the packaging', () => {
  assert.match(script, /missing signature for .*; refusing unsigned updater archive/);
});

test('ambiguous updater archives fail instead of guessing', () => {
  assert.match(script, /multiple updater archives; refusing to choose/);
});

test('updater artifacts keep stable download names', () => {
  assert.match(script, /vesper-macos-\$\{architecture\}\.app\.tar\.gz/);
  assert.match(script, /vesper-macos-\$\{architecture\}\.app\.tar\.gz\.sig/);
});
