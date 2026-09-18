#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { basename, join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const SOURCE = {
  id: 'openslr-104-mucs-hi-en-test',
  name: 'MUCS 2021 Hindi-English code-switching test set',
  url: 'https://www.openslr.org/resources/104/Hindi-English_test.tar.gz',
  revision: 'sha256:93e358b3bf8233a897fcd353c1f4f98fdda6b8c01b7eed17a70c7dd26e984b37',
  license: 'CC-BY-SA-4.0',
  attribution: 'Microsoft Research India; MUCS 2021; distributed by OpenSLR as SLR104',
  redistribution: 'Redistributions must retain attribution and use the same license.',
};

export function parseSegments(text, transcripts) {
  return text
    .trim()
    .split(/\r?\n/u)
    .map((line) => {
      const [id, recording, startText, endText] = line.trim().split(/\s+/u);
      const start = Number(startText);
      const end = Number(endText);
      const reference = transcripts.get(id);
      if (!id || !recording || !Number.isFinite(start) || !Number.isFinite(end) || end <= start) {
        throw new Error(`invalid Kaldi segment: ${line}`);
      }
      if (!reference) throw new Error(`missing transcript for ${id}`);
      return { id, recording, start, end, reference };
    });
}

export function chooseQuickSegments(segments, count) {
  const selected = [];
  const recordings = new Set();
  for (const segment of segments) {
    const duration = segment.end - segment.start;
    if (duration < 4 || duration > 9 || recordings.has(segment.recording)) continue;
    selected.push(segment);
    recordings.add(segment.recording);
    if (selected.length === count) return selected;
  }
  throw new Error(`only found ${selected.length} qualifying recordings; requested ${count}`);
}

function readWav(buffer) {
  if (buffer.toString('ascii', 0, 4) !== 'RIFF' || buffer.toString('ascii', 8, 12) !== 'WAVE') {
    throw new Error('input is not a RIFF/WAVE file');
  }
  let offset = 12;
  let format;
  let data;
  while (offset + 8 <= buffer.length) {
    const id = buffer.toString('ascii', offset, offset + 4);
    const size = buffer.readUInt32LE(offset + 4);
    const body = offset + 8;
    if (body + size > buffer.length) throw new Error(`truncated WAV ${id} chunk`);
    if (id === 'fmt ') {
      format = {
        encoding: buffer.readUInt16LE(body),
        channels: buffer.readUInt16LE(body + 2),
        sampleRate: buffer.readUInt32LE(body + 4),
        byteRate: buffer.readUInt32LE(body + 8),
        blockAlign: buffer.readUInt16LE(body + 12),
        bitsPerSample: buffer.readUInt16LE(body + 14),
      };
    } else if (id === 'data') {
      data = buffer.subarray(body, body + size);
    }
    offset = body + size + (size % 2);
  }
  if (!format || !data) throw new Error('WAV needs fmt and data chunks');
  if (format.encoding !== 1 || format.channels !== 1 || format.bitsPerSample !== 16) {
    throw new Error('MUCS preparation expects mono 16-bit PCM WAV input');
  }
  return { format, data };
}

export function slicePcmWav(buffer, startSeconds, endSeconds) {
  const { format, data } = readWav(buffer);
  const firstFrame = Math.round(startSeconds * format.sampleRate);
  const lastFrame = Math.round(endSeconds * format.sampleRate);
  const firstByte = firstFrame * format.blockAlign;
  const lastByte = Math.min(lastFrame * format.blockAlign, data.length);
  if (firstByte < 0 || firstByte >= lastByte) throw new Error('segment lies outside WAV data');
  const pcm = data.subarray(firstByte, lastByte);
  const output = Buffer.allocUnsafe(44 + pcm.length);
  output.write('RIFF', 0, 'ascii');
  output.writeUInt32LE(36 + pcm.length, 4);
  output.write('WAVEfmt ', 8, 'ascii');
  output.writeUInt32LE(16, 16);
  output.writeUInt16LE(format.encoding, 20);
  output.writeUInt16LE(format.channels, 22);
  output.writeUInt32LE(format.sampleRate, 24);
  output.writeUInt32LE(format.byteRate, 28);
  output.writeUInt16LE(format.blockAlign, 32);
  output.writeUInt16LE(format.bitsPerSample, 34);
  output.write('data', 36, 'ascii');
  output.writeUInt32LE(pcm.length, 40);
  pcm.copy(output, 44);
  return output;
}

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function parseTranscripts(text) {
  return new Map(
    text
      .trim()
      .split(/\r?\n/u)
      .map((line) => {
        const space = line.indexOf(' ');
        if (space < 1) throw new Error(`invalid Kaldi transcript: ${line}`);
        return [line.slice(0, space), line.slice(space + 1).trim()];
      }),
  );
}

async function prepare(inputRoot, outputRoot, count, includeLong) {
  const transcriptRoot = join(inputRoot, 'transcripts');
  const transcripts = parseTranscripts(await readFile(join(transcriptRoot, 'text'), 'utf8'));
  const segments = parseSegments(
    await readFile(join(transcriptRoot, 'segments'), 'utf8'),
    transcripts,
  );
  const chosen = chooseQuickSegments(segments, count);
  const audioRoot = join(outputRoot, 'audio');
  await mkdir(audioRoot, { recursive: true });
  const samples = [];
  for (const segment of chosen) {
    const source = await readFile(join(inputRoot, `${segment.recording}.wav`));
    const audio = slicePcmWav(source, segment.start, segment.end);
    const audioName = `${segment.id}.wav`;
    await writeFile(join(audioRoot, audioName), audio);
    samples.push({
      id: segment.id,
      sourceId: SOURCE.id,
      split: 'test',
      language: 'hi+en',
      audioPath: `audio/${audioName}`,
      sha256: sha256(audio),
      reference: segment.reference,
      durationSeconds: audio.length > 44 ? (audio.length - 44) / 32_000 : 0,
      scenario: 'technical_lecture',
      replayKind: 'natural',
      speakerId: segment.id.split('_', 1)[0],
    });
  }
  if (includeLong) {
    const recording = chosen[0].recording;
    const sourcePath = join(inputRoot, `${recording}.wav`);
    const outputPath = join(audioRoot, `${recording}-full.wav`);
    const source = await readFile(sourcePath);
    const { format, data } = readWav(source);
    await copyFile(sourcePath, outputPath);
    samples.push({
      id: `${recording}-full`,
      sourceId: SOURCE.id,
      split: 'test',
      language: 'hi+en',
      audioPath: `audio/${basename(outputPath)}`,
      sha256: sha256(source),
      reference: segments
        .filter((segment) => segment.recording === recording)
        .sort((left, right) => left.start - right.start)
        .map((segment) => segment.reference)
        .join(' '),
      durationSeconds: data.length / format.byteRate,
      scenario: 'technical_lecture',
      replayKind: 'natural',
      speakerId: chosen[0].id.split('_', 1)[0],
    });
  }
  const manifest = { schemaVersion: 1, sources: [SOURCE], samples };
  await writeFile(join(outputRoot, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(
    `prepared ${samples.length} held-out samples in ${relative(process.cwd(), outputRoot)}`,
  );
}

function usage() {
  return 'usage: node scripts/prepare-mucs-benchmark.mjs EXTRACTED_TEST_DIR OUTPUT_DIR [--count N] [--include-long]';
}

async function main(arguments_) {
  if (arguments_.length < 2) throw new Error(usage());
  let count = 8;
  let includeLong = false;
  for (let index = 2; index < arguments_.length; index += 1) {
    if (arguments_[index] === '--include-long') includeLong = true;
    else if (arguments_[index] === '--count') {
      count = Number(arguments_[++index]);
      if (!Number.isInteger(count) || count < 1 || count > 100)
        throw new Error('--count must be 1..100');
    } else throw new Error(`unknown argument: ${arguments_[index]}`);
  }
  await prepare(resolve(arguments_[0]), resolve(arguments_[1]), count, includeLong);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
