import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { buildSite, landingBenchmarks } from './build-site.mjs';

const catalog = {
  models: [
    { id: 'maker/a', name: 'Maker A' },
    { id: 'maker/b', name: 'Maker B' },
  ],
};
const run = {
  label: '100 held-out FLEURS recordings',
  hardware: 'Synthetic Mac, 16 GB',
  date: '2026-01-02',
};
const evidence = {
  local: {
    runs: { 'en-1': run, 'de-1': { ...run, date: '2026-01-03' } },
    results: {
      'maker/a': {
        en: { run: 'en-1', wer: 5, ms: 40, peakMemoryGb: 1, passed: true, cer: 2 },
        de: {
          run: 'de-1',
          wer: 30,
          ms: 50,
          peakMemoryGb: 1,
          passed: false,
          why: 'more than 20% of words were wrong',
        },
      },
      'maker/b': { en: { run: 'en-1', wer: 4, ms: 900, peakMemoryGb: 2, passed: true } },
    },
    automatic: { 'maker/a': { en: { run: 'en-1', wer: 1, ms: 1, peakMemoryGb: 1, passed: true } } },
  },
  published: {
    'maker/a': {
      en: {
        metric: 'wer',
        value: 6,
        source: 'leaderboard',
        set: 'English average, 1 Jan 2026',
        url: 'https://example.com/board',
      },
    },
    'maker/b': {
      en: {
        metric: 'wer',
        value: 3,
        source: 'maker',
        set: 'Model card',
        url: 'https://example.com/card',
      },
    },
  },
};

test('only passing, explicit-language results are published, with catalog names', () => {
  const data = landingBenchmarks(evidence, catalog);
  assert.deepEqual(
    data.results.map((r) => [r.model, r.language, r.wer]),
    [
      ['Maker B', 'en', 4],
      ['Maker A', 'en', 5],
    ],
  );
  assert.ok(data.results.every((r) => !('cer' in r) && !('run' in r) && !('why' in r)));
  assert.deepEqual(data.languages, { en: 'English' });
});

test('the run envelope is stated only when every published result shares it', () => {
  const data = landingBenchmarks(evidence, catalog);
  assert.equal(data.hardware, 'Synthetic Mac, 16 GB');
  assert.equal(data.recordings, 100);
  assert.equal(data.date, '2026-01-02');
  const mixed = structuredClone(evidence);
  mixed.local.runs['en-2'] = { ...run, hardware: 'Other Mac' };
  mixed.local.results['maker/b'].en.run = 'en-2';
  assert.equal(landingBenchmarks(mixed, catalog).hardware, null);
});

test('published figures are leaderboard averages only, kept apart from measurements', () => {
  const data = landingBenchmarks(evidence, catalog);
  assert.deepEqual(data.published.english, [{ model: 'Maker A', wer: 6 }]);
  assert.match(data.published.note, /^Open ASR Leaderboard, English average/);
});

test('English comes first, then languages by name', () => {
  const many = structuredClone(evidence);
  many.local.results['maker/a'].fr = { run: 'en-1', wer: 7, ms: 40, peakMemoryGb: 1, passed: true };
  many.local.results['maker/a'].de.passed = true;
  // English, then French before German.
  assert.deepEqual(Object.keys(landingBenchmarks(many, catalog).languages), ['en', 'fr', 'de']);
});

test('the assembled site has the page, the font and benchmarks from the real evidence', () => {
  const out = fs.mkdtempSync(path.join(os.tmpdir(), 'vesper-site-'));
  try {
    const data = buildSite(out);
    assert.ok(fs.existsSync(path.join(out, 'index.html')));
    assert.ok(fs.existsSync(path.join(out, 'assets/bricolage-grotesque.ttf')));
    assert.ok(fs.existsSync(path.join(out, 'assets/bricolage-OFL.txt')));
    const written = JSON.parse(fs.readFileSync(path.join(out, 'benchmarks.json'), 'utf8'));
    assert.deepEqual(written, data);
    assert.ok(written.results.length > 0 && written.results.every((r) => Number.isFinite(r.wer)));
    // The site must never claim the release feeds; pages.yml adds those.
    for (const feed of ['latest.json', 'beta.json', 'nightly.json'])
      assert.ok(!fs.existsSync(path.join(out, feed)));
  } finally {
    fs.rmSync(out, { recursive: true, force: true });
  }
});
