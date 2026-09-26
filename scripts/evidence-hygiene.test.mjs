import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
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

function trackedPaths() {
  return execFileSync('git', ['ls-files', '--cached', '-z'], { cwd: root, encoding: 'utf8' })
    .split('\0')
    .filter(Boolean);
}

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

test('local design and build caches are not tracked source', () => {
  const localCache = /(?:^|\/)(?:\.hallmark|\.cache|\.tooling|node_modules)\//;
  const localPreview = /^(?:screenshots|docs\/(?:screenshots|design)|artifacts|design-previews)\//;
  assert.deepEqual(
    trackedPaths().filter((path) => localCache.test(path) || localPreview.test(path)),
    [],
  );
});

test('design caches are ignored by Git and excluded from Docker contexts', async () => {
  const paths = ['.hallmark/preflight.json', '.hallmark/log.json'];
  const ignored = execFileSync('git', ['check-ignore', '--no-index', '--stdin', '-z'], {
    cwd: root,
    encoding: 'utf8',
    input: `${paths.join('\0')}\0`,
  })
    .split('\0')
    .filter(Boolean);
  assert.deepEqual(ignored, paths);
  assert.match(await readFile(join(root, '.dockerignore'), 'utf8'), /^\.hallmark\/?$/m);
});

test('frontend source contains no design-tool critique stamps', async () => {
  const stamp =
    /Hallmark|pre-emit critique|\bP[1-5]\s+H[1-5]\s+E[1-5]\s+S[1-5]\s+R[1-5]\s+V[1-5]\b/i;
  assert.match('/* P5 H4 E5 S5 R5 V4 */', stamp, 'unbranded scores are also metadata');
  assert.doesNotMatch('/* Motion follows application state. */', stamp);
  const sources = trackedPaths().filter(
    (path) =>
      path.startsWith('apps/desktop/src/') && /\.(?:svelte|css|ts|svg|html|m?js)$/.test(path),
  );
  assert.ok(sources.length > 0, 'the frontend source set must not be empty');
  for (const path of sources)
    assert.doesNotMatch(await readFile(join(root, path), 'utf8'), stamp, path);
});
