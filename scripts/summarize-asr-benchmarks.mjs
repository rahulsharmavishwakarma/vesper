#!/usr/bin/env node

import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

function percentile(values, percentile_) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil((percentile_ / 100) * sorted.length) - 1];
}

function median(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

export function summarize(reports) {
  const groups = new Map();
  const comparisonEnvelopes = new Map();
  for (const report of reports) {
    if (report.personalization != null) {
      throw new Error(
        'personalized runs require the separate paired-memory report; they cannot qualify a base model',
      );
    }
    const modelId = report.modelVersions?.asr?.modelId;
    const backend = report.environment?.backend;
    const envelope = JSON.stringify({
      manifestSha256: report.manifestSha256,
      label: report.environment?.label,
      hardware: report.environment?.hardware,
      operatingSystem: report.environment?.operatingSystem,
      powerMode: report.environment?.powerMode,
      precision: report.environment?.precision,
      threads: report.environment?.threads,
      applicationRevision: report.environment?.applicationRevision,
      runtimeRevision: report.environment?.runtimeRevision,
      driver: report.environment?.driver,
    });
    if (
      !modelId ||
      !backend ||
      !report.manifestSha256 ||
      !report.environment?.label ||
      !report.environment?.hardware ||
      !report.environment?.operatingSystem ||
      !report.environment?.powerMode ||
      !report.environment?.precision ||
      !Number.isInteger(report.environment?.threads) ||
      report.environment.threads <= 0 ||
      !report.environment?.applicationRevision ||
      !report.environment?.runtimeRevision ||
      !report.environment?.driver ||
      !Array.isArray(report.runs)
    ) {
      throw new Error('report is missing the model, manifest, execution envelope, or runs');
    }
    for (const run of report.runs) {
      if (!run.language || !run.configuredLanguage || !run.mode) {
        throw new Error('benchmark run is missing language, configured language, or mode');
      }
      const comparisonKey = JSON.stringify([
        run.language,
        run.configuredLanguage,
        run.mode,
        backend,
      ]);
      const previousEnvelope = comparisonEnvelopes.get(comparisonKey);
      if (previousEnvelope && previousEnvelope !== envelope) {
        throw new Error(`incompatible benchmark envelopes for ${comparisonKey}`);
      }
      comparisonEnvelopes.set(comparisonKey, envelope);
      const key = JSON.stringify([
        modelId,
        run.language,
        run.configuredLanguage,
        run.mode,
        backend,
      ]);
      const group = groups.get(key) ?? {
        modelId,
        language: run.language,
        configuredLanguage: run.configuredLanguage,
        mode: run.mode,
        backend,
        wordErrorRates: [],
        realTimeFactors: [],
        stopToFinalMs: [],
        failedSamples: 0,
        pace: run.pace,
      };
      if (group.pace !== run.pace) throw new Error(`incompatible replay pacing for ${key}`);
      if (run.error === null && Number.isFinite(run.rawScore?.wordErrorRate?.rate)) {
        group.wordErrorRates.push(run.rawScore.wordErrorRate.rate);
        if (Number.isFinite(run.realTimeFactor)) group.realTimeFactors.push(run.realTimeFactor);
        if (Number.isFinite(run.stopToFinalMs)) group.stopToFinalMs.push(run.stopToFinalMs);
      } else group.failedSamples += 1;
      groups.set(key, group);
    }
  }
  return [...groups.values()]
    .map((group) => ({
      modelId: group.modelId,
      language: group.language,
      configuredLanguage: group.configuredLanguage,
      mode: group.mode,
      backend: group.backend,
      medianWordErrorRate: median(group.wordErrorRates),
      medianRealTimeFactor: median(group.realTimeFactors),
      p95StopToFinalMs: percentile(group.stopToFinalMs, 95),
      peakMemoryBytes: null,
      successfulSamples: group.wordErrorRates.length,
      failedSamples: group.failedSamples,
    }))
    .sort((left, right) =>
      [left.language, left.configuredLanguage, left.mode, left.backend, left.modelId]
        .join('\0')
        .localeCompare(
          [right.language, right.configuredLanguage, right.mode, right.backend, right.modelId].join(
            '\0',
          ),
        ),
    );
}

async function main(arguments_) {
  if (arguments_.length < 3 || arguments_[0] !== '--out') {
    throw new Error(
      'usage: node scripts/summarize-asr-benchmarks.mjs --out EVIDENCE_JSON REPORT_JSON...',
    );
  }
  const reports = await Promise.all(
    arguments_.slice(2).map(async (path) => JSON.parse(await readFile(path, 'utf8'))),
  );
  const evidence = summarize(reports);
  await writeFile(arguments_[1], `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(`wrote ${evidence.length} comparable evidence group(s) to ${arguments_[1]}`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
