import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { laneName, parseTimeOutput, plannedLanes, SETS } from './run-recommendation-matrix.mjs';

const languages = JSON.parse(readFileSync(new URL('../models/languages.json', import.meta.url)));

test('every declared language gets an explicit lane and detection adds an Automatic lane', () => {
  const lanes = plannedLanes({
    a: { languages: ['en', 'fr'], autoDetect: true },
    b: { languages: ['en'], autoDetect: false },
    writers: { languages: ['en'] },
  });
  assert.deepEqual(
    lanes.map((lane) => `${lane.model}:${lane.set.iso}:${lane.language}`),
    ['a:en:en', 'a:en:auto', 'b:en:en', 'a:fr:fr', 'a:fr:auto'],
  );
});

test('the catalog matrix covers each set with only the models that declare it', () => {
  const lanes = plannedLanes(languages);
  for (const lane of lanes) assert.ok(languages[lane.model].languages.includes(lane.set.iso));
  assert.equal(new Set(lanes.map(laneName)).size, lanes.length);
  assert.ok(lanes.some((lane) => lane.set.iso === 'hi'));
  assert.equal(
    SETS.find((set) => set.iso === 'es').locale,
    'es-419',
    'FLEURS publishes Latin American Spanish only',
  );
});

test('peak memory comes from macOS time -l and is null when missing', () => {
  const text = `   4.20 real 1.00 user 0.50 sys
  1234567  maximum resident set size
  7654321  peak memory footprint`;
  assert.deepEqual(parseTimeOutput(text), {
    maxResidentBytes: 1234567,
    peakFootprintBytes: 7654321,
  });
  assert.deepEqual(parseTimeOutput(''), { maxResidentBytes: null, peakFootprintBytes: null });
});
