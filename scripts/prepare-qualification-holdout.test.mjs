import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chooseUnused } from './prepare-qualification-holdout.mjs';

test('holdout excludes previous audio, same-sentence recordings and normalized references', () => {
  const row = (id, sentence, reference, seconds = 5) => ({
    audioName: `${id}.wav`,
    sentenceId: sentence,
    reference,
    numSamples: seconds * 16000,
  });
  const rows = [
    row('111111111111', 'used', 'First text'),
    row('222222222222', 'used', 'Different translation'),
    row('333333333333', 'ref', 'PRIOR, text!'),
    row('444444444444', 'short', 'Tiny', 1),
    row('555555555555', 'new', 'New sentence'),
    row('666666666666', 'second', 'Another sentence'),
    row('777777777777', 'new', 'New sentence'),
  ];
  const exclusion = { ids: ['111111111111'], references: ['prior text'] };
  const selected = chooseUnused(rows, exclusion, 2);
  assert.deepEqual(new Set(selected.map((r) => r.sentenceId)), new Set(['new', 'second']));
  assert.deepEqual(chooseUnused(rows.toReversed(), exclusion, 2), selected);
  assert.throws(() => chooseUnused(rows, exclusion, 3), /Only 2 unused/);
});
