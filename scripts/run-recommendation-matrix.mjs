#!/usr/bin/env node

// Runs the per-language recommendation matrix (#296): every catalog speech model on each
// held-out FLEURS set through vesper-eval, one process per lane so `/usr/bin/time -l`
// records that lane's peak memory. Explicit-language and Automatic lanes stay separate.

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const SETS = [
  { locale: 'en-US', iso: 'en', dataset: 'fleurs-en-us-100' },
  { locale: 'fr-FR', iso: 'fr', dataset: 'fleurs-fr-fr-100' },
  { locale: 'de-DE', iso: 'de', dataset: 'fleurs-de-de-100' },
  { locale: 'es-419', iso: 'es', dataset: 'fleurs-es-419-100' },
  { locale: 'hi-IN', iso: 'hi', dataset: 'fleurs-hi-in-100' },
];

/** One explicit lane per declared language, plus an Automatic lane where the vendor
 * declares language detection (models/languages.json). */
export function plannedLanes(languages, sets = SETS) {
  const lanes = [];
  for (const set of sets) {
    for (const [model, entry] of Object.entries(languages)) {
      if (model.startsWith('_') || model === 'writers' || !entry.languages?.includes(set.iso))
        continue;
      lanes.push({ model, set, language: set.iso });
      if (entry.autoDetect) lanes.push({ model, set, language: 'auto' });
    }
  }
  return lanes;
}

export function laneName(lane) {
  const mode = lane.language === 'auto' ? 'auto' : 'explicit';
  return `${lane.set.iso}-${mode}-${lane.model.replace(/[^A-Za-z0-9.-]+/gu, '_')}`;
}

/** Peak memory from macOS `/usr/bin/time -l`. The Neural Engine's memory is not charged
 * to the process, so footprint understates engines that run there. */
export function parseTimeOutput(text) {
  const value = (label) => {
    const match = new RegExp(`(\\d+)\\s+${label}`, 'u').exec(text);
    return match ? Number(match[1]) : null;
  };
  return {
    maxResidentBytes: value('maximum resident set size'),
    peakFootprintBytes: value('peak memory footprint'),
  };
}

function usage() {
  return 'usage: node scripts/run-recommendation-matrix.mjs VESPER_EVAL DATASETS_DIR MODEL_ROOT ENVIRONMENT OUTPUT_DIR [--only MODEL_ID] [--resume]';
}

function main(arguments_) {
  if (arguments_.length < 5) throw new Error(usage());
  const [binary, datasets, models, environment, output] = arguments_
    .slice(0, 5)
    .map((value) => resolve(value));
  let only = null;
  let resume = false;
  for (let index = 5; index < arguments_.length; index += 1) {
    if (arguments_[index] === '--only') only = arguments_[++index];
    else if (arguments_[index] === '--resume') resume = true;
    else throw new Error(`unknown argument: ${arguments_[index]}`);
  }
  const languages = JSON.parse(
    readFileSync(new URL('../models/languages.json', import.meta.url), 'utf8'),
  );
  mkdirSync(output, { recursive: true });
  const lanes = plannedLanes(languages).filter((lane) => !only || lane.model === only);
  for (const [index, lane] of lanes.entries()) {
    const name = laneName(lane);
    const report = join(output, `${name}.json`);
    const log = join(output, `${name}.process.json`);
    if (resume && existsSync(log)) continue;
    const root = join(datasets, lane.set.dataset);
    // Power source changes latency on a laptop, so each lane records its own.
    const power = spawnSync('pmset', ['-g', 'batt'], { encoding: 'utf8' }).stdout ?? '';
    const started = Date.now();
    const result = spawnSync(
      '/usr/bin/time',
      [
        '-l',
        binary,
        'run',
        join(root, 'manifest.json'),
        root,
        models,
        environment,
        report,
        '--model',
        lane.model,
        '--language',
        lane.language,
      ],
      { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
    );
    const process = {
      lane: name,
      model: lane.model,
      locale: lane.set.locale,
      configuredLanguage: lane.language,
      exitStatus: result.status,
      wallMs: Date.now() - started,
      powerSource: /'([^']+)'/u.exec(power)?.[1] ?? null,
      reportWritten: existsSync(report),
      ...parseTimeOutput(result.stderr ?? ''),
      stderrTail: (result.stderr ?? '').split('\n').slice(-12).join('\n'),
    };
    writeFileSync(log, `${JSON.stringify(process, null, 2)}\n`);
    console.log(
      `[${index + 1}/${lanes.length}] ${name}: exit ${result.status}, ${Math.round(process.wallMs / 1000)} s, footprint ${process.peakFootprintBytes ?? 'n/a'}`,
    );
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
