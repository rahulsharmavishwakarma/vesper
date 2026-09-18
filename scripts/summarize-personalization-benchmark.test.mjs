import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';

test('paired report preserves regressions, failures and independent locale denominators', async () => {
  const root = await mkdtemp(join(tmpdir(), 'memory-report-'));
  try {
    const metric = (edits) => ({
      wordErrorRate: { edits, referenceUnits: 10 },
      graphemeErrorRate: { edits, referenceUnits: 40 },
    });
    const run = (language, raw, final) => ({
      language,
      sampleId: language,
      rawScore: metric(raw),
      personalizedScore: metric(final),
      rawHypothesis: 'original',
      personalizedHypothesis: 'changed',
      audioSeconds: 2,
      asrMs: 10,
      personalizationMs: 0.1,
      partialUpdates: 0,
      partialEditDistance: 0,
      error: null,
    });
    const report = {
      runs: [
        run('en-US', 2, 1),
        run('en-US', 0, 2),
        {
          language: 'en-US',
          error: 'native failure',
          audioSeconds: 2,
          partialUpdates: 0,
          partialEditDistance: 0,
        },
        run('en-IN', 3, 3),
      ],
    };
    const input = join(root, 'input.json'),
      output = join(root, 'output.json');
    await writeFile(input, JSON.stringify(report));
    execFileSync(process.execPath, [
      resolve('scripts/summarize-personalization-benchmark.mjs'),
      output,
      input,
    ]);
    const { summaries } = JSON.parse(await readFile(output, 'utf8'));
    assert.equal(summaries.length, 2);
    assert.equal(summaries[0].samples, 3);
    assert.equal(summaries[0].scoredSamples, 2);
    assert.equal(summaries[0].failures, 1);
    assert.equal(summaries[0].improved, 1);
    assert.equal(summaries[0].worsened, 1);
    assert.equal(summaries[0].rawWer.percent, 10);
    assert.equal(summaries[0].finalWer.percent, 15);
    assert.equal(summaries[1].rawWer.referenceUnits, 10);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
