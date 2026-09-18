import assert from 'node:assert/strict';
import test from 'node:test';

import { chooseQuickSegments, parseSegments, slicePcmWav } from './prepare-mucs-benchmark.mjs';

function wav(samples = [0, 1, 2, 3]) {
  const output = Buffer.alloc(44 + samples.length * 2);
  output.write('RIFF', 0);
  output.writeUInt32LE(output.length - 8, 4);
  output.write('WAVEfmt ', 8);
  output.writeUInt32LE(16, 16);
  output.writeUInt16LE(1, 20);
  output.writeUInt16LE(1, 22);
  output.writeUInt32LE(2, 24);
  output.writeUInt32LE(4, 28);
  output.writeUInt16LE(2, 32);
  output.writeUInt16LE(16, 34);
  output.write('data', 36);
  output.writeUInt32LE(samples.length * 2, 40);
  samples.forEach((sample, index) => output.writeInt16LE(sample, 44 + index * 2));
  return output;
}

test('parses and selects deterministic distinct-speaker segments', () => {
  const transcripts = new Map([
    ['a', 'one'],
    ['b', 'two'],
    ['c', 'three'],
  ]);
  const segments = parseSegments('a rec-a 0 5\nb rec-a 5 10\nc rec-b 0 8\n', transcripts);
  assert.deepEqual(
    chooseQuickSegments(segments, 2).map((item) => item.id),
    ['a', 'c'],
  );
});

test('slices canonical mono PCM without re-encoding', () => {
  const sliced = slicePcmWav(wav(), 0.5, 1.5);
  assert.equal(sliced.readUInt32LE(24), 2);
  assert.equal(sliced.readUInt32LE(40), 4);
  assert.deepEqual([sliced.readInt16LE(44), sliced.readInt16LE(46)], [1, 2]);
});

test('rejects malformed Kaldi metadata', () => {
  assert.throws(() => parseSegments('bad', new Map()), /invalid Kaldi segment/u);
});
