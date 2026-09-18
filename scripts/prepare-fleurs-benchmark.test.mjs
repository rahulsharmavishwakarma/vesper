import assert from 'node:assert/strict';
import test from 'node:test';

import { chooseFleursRows, parseFleursRows } from './prepare-fleurs-benchmark.mjs';

const row = (sentence, audio, samples, gender = 'FEMALE') =>
  `${sentence}\t${audio}\tRaw reference.\tnormalized reference\tchars\t${samples}\t${gender}`;

test('parses FLEURS rows without replacing the human reference', () => {
  const [parsed] = parseFleursRows(row('10', '123.wav', 160_000));
  assert.equal(parsed.reference, 'Raw reference.');
  assert.equal(parsed.gender, 'female');
});

test('selects deterministic unique sentences in the interactive duration band', () => {
  const rows = parseFleursRows(
    [
      row('1', '1.wav', 32_000),
      row('2', '2.wav', 80_000),
      row('2', '3.wav', 96_000, 'MALE'),
      row('3', '4.wav', 112_000, 'OTHER'),
    ].join('\n'),
  );
  assert.deepEqual(
    chooseFleursRows(rows, 2).map((item) => item.audioName),
    ['2.wav', '4.wav'],
  );
});

test('rejects malformed metadata instead of silently dropping it', () => {
  assert.throws(() => parseFleursRows('bad'), /invalid FLEURS test row/u);
});
