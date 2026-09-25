#!/usr/bin/env node
// Public, cached FLEURS only. Freeze exclusions before opening model outputs.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import {
  createReadStream,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  writeFileSync,
} from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { StringDecoder } from 'node:string_decoder';
import { parseFleursRows } from './prepare-fleurs-benchmark.mjs';

const normalize = (s) =>
  s
    .normalize('NFKC')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();
const digest = (s) => createHash('sha256').update(s).digest('hex');
export function chooseUnused(rows, exclusion, count) {
  const ids = new Set(exclusion.ids),
    references = new Set(exclusion.references);
  const sentences = new Set(
    rows
      .filter((r) => ids.has(r.audioName.slice(0, -4)) || references.has(normalize(r.reference)))
      .map((r) => r.sentenceId),
  );
  const ordered = rows.toSorted((a, b) =>
    digest('qualification-v1:' + a.audioName).localeCompare(
      digest('qualification-v1:' + b.audioName),
    ),
  );
  const selected = [];
  for (const row of ordered) {
    if (
      row.numSamples < 4 * 16000 ||
      row.numSamples > 20 * 16000 ||
      sentences.has(row.sentenceId) ||
      references.has(normalize(row.reference))
    )
      continue;
    selected.push(row);
    sentences.add(row.sentenceId);
    references.add(normalize(row.reference));
    if (selected.length === count) return selected;
  }
  throw Error(`Only ${selected.length} unused eligible sentences; need ${count}`);
}

function* jsonFiles(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isSymbolicLink() || ['.git', 'node_modules', '.venv', 'target'].includes(entry.name))
      continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) yield* jsonFiles(path);
    else if (/\.jsonl?$/.test(entry.name)) yield path;
  }
}

async function collect(root) {
  const ids = new Set(),
    references = new Set(),
    files = [];
  for (const directory of ['benchmarks', '.cache/benchmarks']) {
    for (const file of jsonFiles(join(root, directory))) {
      const hash = createHash('sha256'),
        decoder = new StringDecoder('utf8');
      let tail = '';
      for await (const buffer of createReadStream(file)) {
        hash.update(buffer);
        const text = tail + decoder.write(buffer);
        for (const match of text.matchAll(/\b\d{12,20}\b/g)) ids.add(match[0]);
        for (const match of text.matchAll(
          /"(?:reference|normalizedReference)"\s*:\s*("(?:[^"\\]|\\.)*")/g,
        )) {
          try {
            references.add(normalize(JSON.parse(match[1])));
          } catch {
            /* Partial JSON at chunk boundaries. */
          }
        }
        tail = text.slice(-4096);
      }
      files.push({ path: relative(root, file), sha256: hash.digest('hex') });
    }
  }
  return {
    schemaVersion: 1,
    policy:
      'Exclude known numeric audio IDs, normalized references and every matching sentence group; fixed hash order, unique sentence IDs, 4–20 seconds.',
    ids: [...ids].sort(),
    references: [...references].sort(),
    files,
  };
}

async function main(args) {
  if (args.length < 2 || args.length > 3)
    throw Error(
      'usage: prepare-qualification-holdout.mjs REPOSITORY NEW_OUTPUT [FROZEN_EXCLUSIONS_JSON]',
    );
  const root = resolve(args[0]),
    output = resolve(args[1]);
  if (existsSync(output)) throw Error('Refusing to overwrite a holdout');
  const exclusion = args[2]
    ? JSON.parse(readFileSync(resolve(args[2]), 'utf8'))
    : await collect(root);
  mkdirSync(output, { recursive: true });
  writeFileSync(join(output, 'exclusions.json'), JSON.stringify(exclusion, null, 2) + '\n');
  const preparation = [];
  for (const [locale, folder] of [
    ['en_us', 'en-us'],
    ['hi_in', 'hi-in'],
  ]) {
    const source = JSON.parse(
      readFileSync(join(root, `.cache/benchmarks/datasets/fleurs-${folder}/manifest.json`)),
    ).sources[0];
    const metadata = join(root, `.cache/benchmarks/downloads/fleurs/data/${locale}/test.tsv`);
    const archive = join(
      root,
      `.cache/benchmarks/downloads/fleurs/data/${locale}/audio/test.tar.gz`,
    );
    const expected = (name) =>
      source.revision.match(new RegExp(`${name}-sha256:([a-f0-9]{64})`))?.[1];
    const metadataHash = digest(readFileSync(metadata));
    if (metadataHash !== expected('metadata')) throw Error(`${locale} metadata mismatch`);
    const archiveHash = createHash('sha256');
    for await (const chunk of createReadStream(archive)) archiveHash.update(chunk);
    if (archiveHash.digest('hex') !== expected('audio')) throw Error(`${locale} archive mismatch`);
    const rows = chooseUnused(parseFleursRows(readFileSync(metadata, 'utf8')), exclusion, 32);
    const directory = join(output, locale);
    mkdirSync(directory);
    execFileSync('tar', [
      '-xzf',
      archive,
      '-C',
      directory,
      '--strip-components=1',
      ...rows.map((r) => 'test/' + r.audioName),
    ]);
    const samples = rows.map((row) => {
      const id = `${locale}-${row.audioName.slice(0, -4)}`;
      const path = join(directory, row.audioName),
        data = readFileSync(path);
      renameSync(path, join(directory, id + '.wav'));
      return {
        id,
        sourceId: source.id,
        split: 'test',
        language: locale === 'en_us' ? 'en-US' : 'hi-IN',
        audioPath: `${locale}/${id}.wav`,
        sha256: digest(data),
        reference: row.reference,
        durationSeconds: row.numSamples / 16000,
        scenario: 'read',
        replayKind: 'natural',
      };
    });
    const manifest = { schemaVersion: 1, sources: [source], samples };
    const text = JSON.stringify(manifest, null, 2) + '\n';
    writeFileSync(join(output, locale + '.json'), text);
    preparation.push({
      locale,
      count: samples.length,
      manifestSha256: digest(text),
      metadataSha256: metadataHash,
      sentenceIds: rows.map((r) => r.sentenceId),
      genders: rows.map((r) => r.gender),
    });
  }
  writeFileSync(
    join(output, 'preparation.json'),
    JSON.stringify(
      {
        schemaVersion: 1,
        preparation,
        exclusionSha256: digest(readFileSync(join(output, 'exclusions.json'))),
        limitations: [
          'Unused relative to the frozen scanned public manifests/reports, not proof of exclusion from model training or unavailable experiment history.',
          'Natural read speech; speaker independence, physical microphone, spontaneous dictation, code switching and singing are not established.',
        ],
      },
      null,
      2,
    ) + '\n',
  );
  console.log(JSON.stringify(preparation));
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  await main(process.argv.slice(2));
