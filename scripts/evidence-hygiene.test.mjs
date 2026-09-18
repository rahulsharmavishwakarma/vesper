import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const generatedResultDirectories = ['benchmarks/asr/results', 'benchmarks/cleanup/results'];
const validationInputs = new Set([
  'audio-frontend-candidates.json',
  'audio-frontend-recency.json',
  'audio-quality-research.json',
  'audio-regression-boundary-sources.json',
  'cleanup-cases.json',
]);

async function jsonFiles(relativeDirectory) {
  try {
    return (await readdir(join(root, relativeDirectory))).filter((name) => name.endsWith('.json'));
  } catch (error) {
    if (error.code === 'ENOENT') return [];
    throw error;
  }
}

test('generated benchmark reports stay out of source control paths', async () => {
  for (const directory of generatedResultDirectories) {
    assert.deepEqual(await jsonFiles(directory), [], `${directory} must contain no JSON reports`);
  }
});

test('docs validation JSON is limited to reviewed inputs and source inventories', async () => {
  const actual = new Set(await jsonFiles('docs/validation'));
  assert.deepEqual(actual, validationInputs);
});

test('generated report directories are ignored', async () => {
  const ignore = await readFile(join(root, '.gitignore'), 'utf8');
  for (const directory of generatedResultDirectories) {
    assert.match(ignore, new RegExp(`^/${directory}/$`, 'm'));
  }
});
