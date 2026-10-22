import assert from 'node:assert/strict';
import test from 'node:test';
import { exportEvidence, laneResult } from './export-recommendation-evidence.mjs';

const clip = (edits, referenceUnits, extra = {}) => ({
  asrMs: 40,
  realTimeFactor: 0.01,
  rawScore: {
    wordErrorRate: { edits, referenceUnits, rate: edits / referenceUnits },
    graphemeErrorRate: {
      edits,
      referenceUnits: referenceUnits * 5,
      rate: edits / (referenceUnits * 5),
    },
  },
  ...extra,
});
const report = (runs, kind = 'parakeet_tdt') => ({
  runs,
  modelVersions: { asr: { adapter: { kind } } },
});

test('corpus WER counts words, and failures stay in the denominator', () => {
  const result = laneResult({
    report: report([clip(1, 10), clip(3, 30), { error: 'decoder failed' }]),
    process: { peakFootprintBytes: 1_234_000_000 },
    planned: 3,
    run: 'en-fleurs-100',
  });
  assert.equal(result.wer, 10);
  assert.equal(result.executions, 3);
  assert.equal(result.failures, 1);
  assert.equal(result.passed, false);
  assert.equal(result.why, '1 of 3 recordings failed');
  assert.equal(result.peakMemoryGb, 1.23);
  assert.equal(result.engine, 'parakeet_tdt');
});

test('a lane that wrote no report fails every planned recording', () => {
  const result = laneResult({ report: null, process: {}, planned: 100, run: 'fr-fleurs-100' });
  assert.deepEqual(
    [result.wer, result.failures, result.passed, result.why],
    [null, 100, false, '100 of 100 recordings failed'],
  );
});

test('the accuracy and speed gates explain why a lane did not pass', () => {
  const inaccurate = laneResult({
    report: report(Array.from({ length: 5 }, () => clip(3, 10))),
    process: {},
    planned: 5,
    run: 'hi-fleurs-100',
  });
  assert.equal(inaccurate.why, 'more than 20% of words were wrong');
  const slow = laneResult({
    report: report([clip(0, 10, { realTimeFactor: 1.5 })]),
    process: {},
    planned: 1,
    run: 'en-fleurs-100',
  });
  assert.equal(slow.why, 'slower than real time');
  const good = laneResult({ report: report([clip(0, 10)]), process: {}, planned: 1, run: 'x' });
  assert.equal(good.passed, true);
  assert.equal('why' in good, false);
});

test('export replaces only the measured section and keeps Automatic lanes apart', () => {
  const evidence = { writers: { w: {} }, published: { p: {} }, local: { runs: { old: {} } } };
  const exported = exportEvidence(
    evidence,
    [
      {
        model: 'm',
        iso: 'en',
        automatic: false,
        planned: 1,
        report: report([clip(0, 10)]),
        process: {},
      },
      {
        model: 'm',
        iso: 'en',
        automatic: true,
        planned: 1,
        report: report([clip(1, 10)]),
        process: {},
      },
    ],
    { date: '2026-10-22', hardware: 'Apple M5 Pro, 64 GB', source: 'docs/x.md' },
  );
  assert.deepEqual(exported.writers, evidence.writers);
  assert.deepEqual(exported.published, evidence.published);
  assert.deepEqual(Object.keys(exported.local.runs), ['en-fleurs-100']);
  assert.equal(exported.local.results.m.en.wer, 0);
  assert.equal(exported.local.automatic.m.en.wer, 10);
});
