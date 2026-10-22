#!/usr/bin/env node

// Turns the per-language recommendation matrix (#296) into the `local` section of
// models/recommendations.json: aggregates only, never audio, transcripts or paths.
// Failed recordings and crashed lanes stay in the denominator (ADR-0010).

import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

/** Screening gates from benchmarks/asr/request.*.example.json. */
export const GATES = { medianWer: 0.2, failureRate: 0.02, medianRealTimeFactor: 1 };

const median = (values) => {
  const sorted = values.filter((value) => Number.isFinite(value)).sort((a, b) => a - b);
  if (!sorted.length) return null;
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const round = (value, digits) => (value === null ? null : Number(value.toFixed(digits)));
const corpusRate = (runs, key) => {
  const scores = runs.map((run) => run.rawScore?.[key]).filter(Boolean);
  const units = scores.reduce((total, score) => total + score.referenceUnits, 0);
  return units ? scores.reduce((total, score) => total + score.edits, 0) / units : null;
};

/** One lane's result. `report` is null when the process wrote none; then every
 * planned recording counts as a failure. */
export function laneResult({ report, process, planned, run }) {
  const runs = report?.runs ?? [];
  const finished = runs.filter((entry) => !entry.error && entry.rawScore?.wordErrorRate);
  const executions = Math.max(planned, runs.length);
  const failures = executions - finished.length;
  const medianWer = median(finished.map((entry) => entry.rawScore.wordErrorRate.rate));
  const medianRtf = median(finished.map((entry) => entry.realTimeFactor));
  const wer = corpusRate(finished, 'wordErrorRate');
  const cer = corpusRate(finished, 'graphemeErrorRate');
  const why =
    failures / executions > GATES.failureRate
      ? `${failures} of ${executions} recordings failed`
      : medianWer === null || medianWer > GATES.medianWer
        ? 'more than 20% of words were wrong'
        : medianRtf === null || medianRtf > GATES.medianRealTimeFactor
          ? 'slower than real time'
          : null;
  return {
    run,
    wer: wer === null ? null : round(wer * 100, 2),
    cer: cer === null ? null : round(cer * 100, 2),
    ms: round(median(finished.map((entry) => entry.asrMs)), 0),
    // The Neural Engine's memory is not charged to the process, so this
    // understates engines that run there; `engine` says which one ran.
    peakMemoryGb:
      process?.peakFootprintBytes == null ? null : round(process.peakFootprintBytes / 1e9, 2),
    engine: report?.modelVersions?.asr?.adapter?.kind ?? null,
    executions,
    failures,
    passed: why === null,
    ...(why ? { why } : {}),
  };
}

const LOCALES = {
  en: ['en-fleurs-100', 'English'],
  fr: ['fr-fleurs-100', 'French'],
  de: ['de-fleurs-100', 'German'],
  es: ['es-fleurs-100', 'Latin American Spanish'],
  hi: ['hi-fleurs-100', 'Hindi'],
};

/** Replace the measured section; published figures and writers are untouched. */
export function exportEvidence(evidence, lanes, { date, hardware, source }) {
  const runs = {};
  const results = {};
  const automatic = {};
  for (const lane of lanes) {
    const [run, name] = LOCALES[lane.iso];
    runs[run] = { label: `100 held-out FLEURS ${name} recordings`, hardware, date, source };
    const target = lane.automatic ? automatic : results;
    (target[lane.model] ??= {})[lane.iso] = laneResult({ ...lane, run });
  }
  return { ...evidence, local: { runs, results, automatic } };
}

function main([matrix, evidencePath, date, hardware, source]) {
  if (!source)
    throw new Error(
      'usage: node scripts/export-recommendation-evidence.mjs MATRIX_DIR RECOMMENDATIONS_JSON DATE HARDWARE SOURCE_DOC',
    );
  const lanes = readdirSync(resolve(matrix))
    .filter((file) => file.endsWith('.process.json'))
    .map((file) => {
      const process = JSON.parse(readFileSync(join(matrix, file), 'utf8'));
      const reportPath = join(matrix, file.replace('.process.json', '.json'));
      const report = process.reportWritten ? JSON.parse(readFileSync(reportPath, 'utf8')) : null;
      return {
        model: process.model,
        iso: process.locale.slice(0, 2),
        automatic: process.configuredLanguage === 'auto',
        planned: 100,
        report,
        process,
      };
    });
  const evidence = JSON.parse(readFileSync(evidencePath, 'utf8'));
  writeFileSync(
    evidencePath,
    `${JSON.stringify(exportEvidence(evidence, lanes, { date, hardware, source }), null, 2)}\n`,
  );
  console.log(`exported ${lanes.length} lanes into ${evidencePath}`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
