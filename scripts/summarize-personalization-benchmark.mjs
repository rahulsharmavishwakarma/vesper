#!/usr/bin/env node
// Paired within-recording scores; never combines languages, profiles or runtime envelopes.
import { readFile, writeFile } from 'node:fs/promises';
import { basename } from 'node:path';

const percentile = (values, p) => {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.ceil(p * sorted.length) - 1];
};
const rate = (runs, field, metric) => {
  const scores = runs.map((r) => r[field]?.[metric]).filter(Boolean);
  const edits = scores.reduce((n, s) => n + s.edits, 0);
  const units = scores.reduce((n, s) => n + s.referenceUnits, 0);
  return { edits, referenceUnits: units, percent: units ? (100 * edits) / units : null };
};
const [output, ...paths] = process.argv.slice(2);
if (!output || !paths.length)
  throw Error('usage: summarize-personalization-benchmark.mjs OUTPUT REPORT...');
const summaries = [];
for (const path of paths) {
  const report = JSON.parse(await readFile(path, 'utf8'));
  for (const language of [...new Set(report.runs.map((r) => r.language))]) {
    const runs = report.runs.filter((r) => r.language === language);
    const scored = runs.filter((r) => r.rawScore && r.personalizedScore);
    const before = (r) => r.cleanedScore ?? r.rawScore;
    const changed = scored.filter(
      (r) => r.personalizedHypothesis !== (r.cleanedHypothesis ?? r.rawHypothesis),
    );
    const examples = changed.map((r) => ({
      id: r.sampleId,
      reference: r.rawScore.normalizedReference,
      raw: r.rawHypothesis,
      cleaned: r.cleanedHypothesis,
      personalized: r.personalizedHypothesis,
      beforeEdits: before(r).wordErrorRate.edits,
      afterEdits: r.personalizedScore.wordErrorRate.edits,
      memory: r.personalizationEvidence,
    }));
    summaries.push({
      report: basename(path),
      language,
      manifestSha256: report.manifestSha256,
      profileSha256: report.personalizationSha256,
      environment: report.environment,
      samples: runs.length,
      failures: runs.filter((r) => r.error).length,
      scoredSamples: scored.length,
      audioSeconds: runs.reduce((n, r) => n + r.audioSeconds, 0),
      rawWer: rate(runs, 'rawScore', 'wordErrorRate'),
      cleanedWer: rate(runs, 'cleanedScore', 'wordErrorRate'),
      finalWer: rate(runs, 'personalizedScore', 'wordErrorRate'),
      rawCer: rate(runs, 'rawScore', 'graphemeErrorRate'),
      finalCer: rate(runs, 'personalizedScore', 'graphemeErrorRate'),
      changed: changed.length,
      improved: scored.filter(
        (r) => r.personalizedScore.wordErrorRate.edits < before(r).wordErrorRate.edits,
      ).length,
      worsened: scored.filter(
        (r) => r.personalizedScore.wordErrorRate.edits > before(r).wordErrorRate.edits,
      ).length,
      appliedOccurrences: runs.reduce(
        (n, r) =>
          n + (r.personalizationEvidence?.applied ?? []).reduce((m, x) => m + x.occurrences, 0),
        0,
      ),
      asrMsP50: percentile(
        runs.filter((r) => !r.error).map((r) => r.asrMs),
        0.5,
      ),
      asrMsP95: percentile(
        runs.filter((r) => !r.error).map((r) => r.asrMs),
        0.95,
      ),
      firstPartialMsP50: percentile(
        runs.map((r) => r.firstPartialMs).filter((x) => x != null),
        0.5,
      ),
      stopToFinalMsP50: percentile(
        runs.map((r) => r.stopToFinalMs).filter((x) => x != null),
        0.5,
      ),
      stopToFinalMsP95: percentile(
        runs.map((r) => r.stopToFinalMs).filter((x) => x != null),
        0.95,
      ),
      memoryMsP95: percentile(
        runs.map((r) => r.personalizationMs).filter((x) => x != null),
        0.95,
      ),
      partialUpdates: runs.reduce((n, r) => n + r.partialUpdates, 0),
      partialEditDistance: runs.reduce((n, r) => n + r.partialEditDistance, 0),
      examples,
    });
  }
}
await writeFile(output, JSON.stringify({ schemaVersion: 1, summaries }, null, 2) + '\n');
for (const s of summaries)
  console.log(
    `${s.report} ${s.language}: n=${s.samples} failures=${s.failures} WER ${s.rawWer.percent?.toFixed(2)} -> ${s.finalWer.percent?.toFixed(2)} changed=${s.changed} improved=${s.improved} worsened=${s.worsened}`,
  );
