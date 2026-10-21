import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

const revision = '97a15afa5caa9bce5baaa86c1184103877af4101';
const prefix = 'libvesper_diar_97a15afa_';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'vesper-diar-stage-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'scripts'));
  mkdirSync(join(root, 'crates/vesper-models/src'), { recursive: true });
  copyFileSync(
    new URL('./diarization-runtime.mjs', import.meta.url),
    join(root, 'scripts/diarization-runtime.mjs'),
  );
  copyFileSync(
    new URL('./macho-minimum.mjs', import.meta.url),
    join(root, 'scripts/macho-minimum.mjs'),
  );
  writeFileSync(
    join(root, 'crates/vesper-models/src/diarization.rs'),
    `RUNTIME_REVISION: &str = "${revision}"`,
  );
  // A sentinel prevents network/native builds and proves strict-mode dispatch.
  writeFileSync(
    join(root, 'scripts/build-diarization-runtime.mjs'),
    'export function buildDiarizationRuntime() { throw new Error("BOOTSTRAP_CALLED"); }',
  );
  const stage = join(root, '.tooling/diarization-runtime/nemotron-diarization-runtime', revision);
  const models = join(root, 'models/downloads');
  const run = (args = []) =>
    spawnSync(process.execPath, [join(root, 'scripts/diarization-runtime.mjs'), ...args], {
      encoding: 'utf8',
      env: {
        ...process.env,
        // The script also searches the installed app's data folder under HOME;
        // isolate it so a developer's real runtime cannot satisfy the fixture.
        HOME: root,
        NEMO_SPEECH_SOURCE: '',
        VESPER_MODEL_DIR: models,
        UKTI_MODEL_DIR: '',
        STT_MODEL_DIR: '',
      },
    });
  return { root, stage, models, run };
}

function bundle(path) {
  mkdirSync(path, { recursive: true });
  const files = ['api', 'asr', 'ggml', 'base', 'cpu', 'blas', 'metal'].map((component) => {
    const name = `${prefix}${component}.dylib`;
    const bytes = Buffer.from(`synthetic fixture ${component}`);
    writeFileSync(join(path, name), bytes);
    return {
      name,
      sizeBytes: bytes.length,
      sha256: createHash('sha256').update(bytes).digest('hex'),
    };
  });
  writeFileSync(join(path, 'manifest.json'), JSON.stringify({ sourceRevision: revision, files }));
}

test('verified staging allows strict offline builds without native bootstrap', (t) => {
  const { stage, run } = fixture(t);
  bundle(stage);
  const result = run(['--strict']);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /staging is current/);
});

test('verified installed bundle repairs tampered staging before any source build', (t) => {
  const { stage, models, run } = fixture(t);
  bundle(stage);
  writeFileSync(join(stage, `${prefix}api.dylib`), 'tampered');
  const installed = join(models, 'nemotron-diarization-runtime', revision);
  bundle(installed);
  const manifest = readFileSync(join(installed, 'manifest.json'));
  const result = run(['--strict']);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Staged verified/);
  assert.deepEqual(
    readFileSync(join(stage, `${prefix}api.dylib`)),
    readFileSync(join(installed, `${prefix}api.dylib`)),
  );
  assert.deepEqual(readFileSync(join(installed, 'manifest.json')), manifest);
});

test('absent runtime bootstraps only strict builds; development warns and continues', (t) => {
  const { run } = fixture(t);
  const development = run();
  assert.equal(development.status, 0, development.stderr);
  assert.match(development.stderr, /speaker timelines stay unavailable/);
  const build = run(['--strict']);
  assert.notEqual(build.status, 0);
  assert.match(build.stderr, /BOOTSTRAP_CALLED/);
});

test('a corrupt installed bundle is never accepted as ready', (t) => {
  const { models, run } = fixture(t);
  const installed = join(models, 'nemotron-diarization-runtime', revision);
  bundle(installed);
  writeFileSync(join(installed, `${prefix}metal.dylib`), 'tampered');
  const result = run(['--strict']);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /BOOTSTRAP_CALLED/);
});
