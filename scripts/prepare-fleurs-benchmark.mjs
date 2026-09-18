#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { basename, extname, join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const REVISION = '70bb2e84b976b7e960aa89f1c648e09c59f894dd';
const LOCALES = {
  hi_in: {
    language: 'hi-IN',
    name: 'FLEURS Hindi (India) test set',
    audioSha256: '47c238b9cf7016ba596639bb3038b0c89dc6e85759f16d09c4f58c6dfa25c65f',
    metadataSha256: '889e82e2875490f9533ff59ab67326fcfa01cf828fc4412758aaaa1f442a729f',
  },
  en_us: {
    language: 'en-US',
    name: 'FLEURS English (United States) test set',
    audioSha256: 'd9c2e37b41aacd41bc283554a0a82b5476b36887049774ecb2819dcaaa55a356',
    metadataSha256: '74c046239374deeb60fa63f258f907388093a32bcaa3140965f70ef05c79f7ca',
  },
};

export function parseFleursRows(text) {
  return text
    .trim()
    .split(/\r?\n/u)
    .map((line) => {
      const fields = line.split('\t');
      const sentenceId = fields[0];
      const audioName = fields[1];
      const reference = fields[2];
      const numSamples = Number(fields[5]);
      const gender = fields[6]?.toLowerCase();
      if (
        !sentenceId ||
        !/^\d+\.wav$/u.test(audioName) ||
        !reference ||
        !Number.isInteger(numSamples) ||
        numSamples <= 0 ||
        !['female', 'male', 'other'].includes(gender)
      ) {
        throw new Error(`invalid FLEURS test row: ${line}`);
      }
      return { sentenceId, audioName, reference, numSamples, gender };
    });
}

export function chooseFleursRows(rows, count) {
  const selected = [];
  const sentences = new Set();
  for (const row of rows) {
    const duration = row.numSamples / 16_000;
    if (duration < 4 || duration > 20 || sentences.has(row.sentenceId)) continue;
    selected.push(row);
    sentences.add(row.sentenceId);
    if (selected.length === count) return selected;
  }
  throw new Error(`only found ${selected.length} qualifying unique sentences; requested ${count}`);
}

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

async function prepare(localeId, audioRoot, metadataPath, outputRoot, count) {
  const locale = LOCALES[localeId];
  if (!locale) throw new Error(`unsupported FLEURS locale: ${localeId}`);
  const rows = chooseFleursRows(parseFleursRows(await readFile(metadataPath, 'utf8')), count);
  const outputAudio = join(outputRoot, 'audio');
  await mkdir(outputAudio, { recursive: true });
  const samples = [];
  for (const row of rows) {
    const sourcePath = join(audioRoot, row.audioName);
    const audio = await readFile(sourcePath);
    const audioName = `${localeId}-${basename(row.audioName, extname(row.audioName))}.wav`;
    await copyFile(sourcePath, join(outputAudio, audioName));
    samples.push({
      id: basename(audioName, '.wav'),
      sourceId: `fleurs-${localeId}-test`,
      split: 'test',
      language: locale.language,
      audioPath: `audio/${audioName}`,
      sha256: sha256(audio),
      reference: row.reference,
      durationSeconds: row.numSamples / 16_000,
      scenario: 'read',
      replayKind: 'natural',
    });
  }
  const source = {
    id: `fleurs-${localeId}-test`,
    name: locale.name,
    url: `https://huggingface.co/datasets/google/fleurs/tree/${REVISION}/data/${localeId}`,
    revision: `${REVISION}; audio-sha256:${locale.audioSha256}; metadata-sha256:${locale.metadataSha256}`,
    license: 'CC-BY-4.0',
    attribution: 'Google FLEURS; derived from the FLoRes evaluation sentences',
    redistribution: 'Redistributions must retain attribution under CC-BY-4.0.',
  };
  await writeFile(
    join(outputRoot, 'manifest.json'),
    `${JSON.stringify({ schemaVersion: 1, sources: [source], samples }, null, 2)}\n`,
  );
  console.log(
    `prepared ${samples.length} ${locale.language} samples in ${relative(process.cwd(), outputRoot)}`,
  );
}

function usage() {
  return 'usage: node scripts/prepare-fleurs-benchmark.mjs hi_in|en_us EXTRACTED_TEST_DIR TEST_TSV OUTPUT_DIR [--count N]';
}

async function main(arguments_) {
  if (arguments_.length < 4) throw new Error(usage());
  let count = 16;
  for (let index = 4; index < arguments_.length; index += 1) {
    if (arguments_[index] !== '--count') throw new Error(`unknown argument: ${arguments_[index]}`);
    count = Number(arguments_[++index]);
    if (!Number.isInteger(count) || count < 1 || count > 100)
      throw new Error('--count must be 1..100');
  }
  await prepare(
    arguments_[0],
    resolve(arguments_[1]),
    resolve(arguments_[2]),
    resolve(arguments_[3]),
    count,
  );
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
