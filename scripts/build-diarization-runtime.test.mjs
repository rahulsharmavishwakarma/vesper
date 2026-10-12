import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { buildDiarizationRuntime } from './build-diarization-runtime.mjs';

const revision = '97a15afa5caa9bce5baaa86c1184103877af4101';
const sentencepiece = '3863f7648e5d8edb571ac592f3ac4f5f0695275a';

function fixture(t, { wrongRevision = false, dirty = false, failBuild = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'vesper-diar-build-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const calls = [];
  const heads = new Map();
  const run = (command, args, options = {}) => {
    calls.push({ command, args, cwd: options.cwd });
    if (command === 'git' && args[0] === 'init') mkdirSync(join(args[1], '.git'));
    if (command === 'git' && args[0] === 'checkout')
      heads.set(options.cwd, options.cwd.endsWith('sentencepiece') ? sentencepiece : revision);
    if (command === 'git' && args[0] === 'rev-parse') {
      if (!heads.has(options.cwd)) throw new Error('No HEAD yet');
      return wrongRevision ? 'unexpected-revision' : heads.get(options.cwd);
    }
    if (command === 'git' && args[0] === 'diff') return dirty ? 'modified source' : '';
    if (failBuild && command === 'cmake') throw new Error('compiler failed');
    return '';
  };
  const options = { platform: 'darwin', arch: 'arm64', environment: {}, run };
  return { root, calls, options };
}

test('fresh bootstrap fetches immutable sources and statically builds only the speaker dependency', (t) => {
  const { root, calls, options } = fixture(t);
  assert.equal(
    buildDiarizationRuntime(root, revision, options),
    join(root, '.tooling/nemo-speech.cpp'),
  );
  assert.deepEqual(
    calls.filter((c) => c.command === 'git' && c.args[0] === 'fetch').map((c) => c.args.at(-1)),
    [revision, sentencepiece],
  );
  assert.ok(calls.some((c) => c.args.includes('submodule') && c.args.at(-1) === 'ggml'));
  const configure = calls.find((c) => c.command === 'bash');
  assert.ok(configure.args.includes('metal-diar'));
  assert.ok(configure.args.includes('-DNEMO_SPEECH_BUILD_MIC_CAPTURE=OFF'));
  const dependency = calls.find((c) => c.command === 'cmake' && c.args[0] === '-S');
  assert.ok(dependency.args.includes('-DSPM_ENABLE_SHARED=OFF'));
  assert.ok(dependency.args.includes('-DSPM_ENABLE_TCMALLOC=OFF'));
  assert.equal(calls.filter((c) => c.args[0] === '--build').length, 2);
  assert.ok(calls.filter((c) => c.args[0] === '--build').every((c) => c.args.at(-1) === '4'));
});

test('cached checkouts are reused without fetching, resetting or cleaning local files', (t) => {
  const { root, calls, options } = fixture(t);
  buildDiarizationRuntime(root, revision, options);
  mkdirSync(join(root, '.tooling/nemo-speech.cpp/ggml/CMakeLists.txt'), { recursive: true });
  calls.length = 0;
  buildDiarizationRuntime(root, revision, options);
  assert.ok(
    !calls.some((c) => ['fetch', 'checkout', 'reset', 'clean', 'submodule'].includes(c.args[0])),
  );
});

for (const [name, settings, message] of [
  ['wrong revision', { wrongRevision: true }, /Unexpected source revision/],
  ['modified source', { dirty: true }, /local changes/],
  ['failed compiler', { failBuild: true }, /compiler failed/],
]) {
  test(`bootstrap stops on ${name} before native configuration or packaging`, (t) => {
    const { root, calls, options } = fixture(t, settings);
    assert.throws(() => buildDiarizationRuntime(root, revision, options), message);
    assert.ok(!calls.some((c) => c.command === 'bash' || c.command === 'uv'));
  });
}

test('unsupported hosts and invalid job limits fail before source mutation', (t) => {
  const { root, calls, options } = fixture(t);
  assert.throws(
    () => buildDiarizationRuntime(root, revision, { ...options, arch: 'x64' }),
    /Apple silicon/,
  );
  assert.throws(
    () =>
      buildDiarizationRuntime(root, revision, {
        ...options,
        environment: { CMAKE_BUILD_PARALLEL_LEVEL: '0' },
      }),
    /positive integer/,
  );
  assert.deepEqual(calls, []);
});
