import assert from 'node:assert/strict';
import test from 'node:test';
import { classifyEntry, findViolations } from './check-no-private-keys.mjs';

test('clean source and the enrolled public key pass', () => {
  assert.equal(classifyEntry('src/app.mjs', 'export const x = 1;\n'), null);
  assert.equal(
    classifyEntry(
      'tauri.conf.json',
      '{"pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFCQ0RFRgo="}',
    ),
    null,
  );
  assert.equal(classifyEntry('keys/vesper.key.pub', 'public half'), null);
});

test('secret content markers are refused', () => {
  for (const marker of [
    'untrusted comment: minisign encrypted secret key: ABC\n',
    '-----BEGIN PRIVATE KEY-----\n',
    '-----BEGIN RSA PRIVATE KEY-----\n',
    '-----BEGIN EC PRIVATE KEY-----\n',
    '-----BEGIN OPENSSH PRIVATE KEY-----\n',
  ]) {
    assert.match(classifyEntry('notes.txt', `prefix ${marker} suffix`), /private key material/);
  }
});

test('key and password filenames are refused, public halves allowed', () => {
  assert.match(classifyEntry('secrets/vesper.key', 'anything'), /private key filename/);
  assert.match(classifyEntry('id_rsa', 'anything'), /SSH private key filename/);
  assert.match(classifyEntry('backup/vesper.key.password', 'anything'), /password filename/);
  assert.equal(classifyEntry('keys/vesper.key.pub', 'anything'), null);
  assert.equal(classifyEntry('id_rsa.pub', 'anything'), null);
  assert.equal(classifyEntry('src/keyboard.mjs', 'anything'), null);
});

test('findViolations reports every offender and nothing else', () => {
  const violations = findViolations([
    { file: 'ok.mjs', content: 'fine' },
    { file: 'leak/vesper.key', content: 'bytes' },
    { file: 'notes.txt', content: 'has minisign encrypted secret key inside' },
  ]);
  assert.equal(violations.length, 2);
  assert.ok(violations[0].includes('leak/vesper.key'));
  assert.ok(violations[1].includes('notes.txt'));
});
