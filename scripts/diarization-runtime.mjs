#!/usr/bin/env node
// Stage the pinned speaker (diarization) runtime for application bundles.
// No model downloads. Sources, in order: an already-current staging
// directory, a fully hash-verified copy from a local model directory, or a
// pinned native source checkout via the packager. Strict bundle builds prepare
// the pinned source when needed; otherwise the staging directory
// is left present but empty so development continues without the stage.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildDiarizationRuntime } from './build-diarization-runtime.mjs';
import { loadableOn, minimumSystem } from './macho-minimum.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const strict = process.argv.includes('--strict');
if (process.argv.some((arg) => arg !== '--strict' && arg.startsWith('--')))
  throw new Error('usage: node scripts/diarization-runtime.mjs [--strict]');

const STAGING = join(root, '.tooling/diarization-runtime/nemotron-diarization-runtime');
const COMPONENTS = ['api', 'asr', 'ggml', 'base', 'cpu', 'blas', 'metal'];
const PREFIX = 'libvesper_diar_97a15afa_';

const rust = readFileSync(join(root, 'crates/vesper-models/src/diarization.rs'), 'utf8');
const REVISION = rust.match(/RUNTIME_REVISION: &str = "([0-9a-f]{40})"/)?.[1];
if (!REVISION) throw new Error('Cannot read pinned diarization RUNTIME_REVISION');

const sha256 = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');

function verifiedBundle(directory) {
  const manifestPath = join(directory, 'manifest.json');
  if (!existsSync(manifestPath) || readFileSync(manifestPath).length > 16_384) return false;
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  } catch {
    return false;
  }
  if (manifest.sourceRevision !== REVISION || !Array.isArray(manifest.files)) return false;
  const expected = new Set(COMPONENTS.map((c) => `${PREFIX}${c}.dylib`));
  const found = new Set(manifest.files.map((f) => f.name));
  if (found.size !== expected.size || manifest.files.length !== 7) return false;
  if ([...expected].some((name) => !found.has(name))) return false;
  return manifest.files.every(
    (f) =>
      f.sizeBytes > 0 &&
      /^[0-9a-f]{64}$/.test(f.sha256 ?? '') &&
      existsSync(join(directory, f.name)) &&
      sha256(join(directory, f.name)) === f.sha256 &&
      readFileSync(join(directory, f.name)).length === f.sizeBytes &&
      loadableOn(minimumSystem(join(directory, f.name)) ?? '0'),
  );
}

function current() {
  return verifiedBundle(join(STAGING, REVISION));
}

function stageFrom(source) {
  const from = join(source, REVISION);
  if (!existsSync(from) || !verifiedBundle(from)) return false;
  rmSync(STAGING, { recursive: true, force: true });
  mkdirSync(STAGING, { recursive: true });
  cpSync(from, join(STAGING, REVISION), { recursive: true });
  return current();
}

const nativeSource = () => {
  const candidates = [
    process.env.NEMO_SPEECH_SOURCE,
    join(root, '.tooling/nemo-speech.cpp'),
    join(root, '..', 'NeMo-Speech.cpp'),
  ].filter(Boolean);
  return candidates.find((path) => existsSync(join(path, 'ggml')));
};

const modelRoots = () => {
  const roots = [join(root, 'models/downloads')];
  for (const variable of ['VESPER_MODEL_DIR', 'UKTI_MODEL_DIR', 'STT_MODEL_DIR']) {
    if (process.env[variable]) roots.push(process.env[variable]);
  }
  const data = join(homedir(), 'Library/Application Support/com.vesper.desktop');
  roots.push(join(data, 'models'));
  return [...new Set(roots)].filter((path) => existsSync(path));
};

if (current()) {
  console.log(`Diarization runtime staging is current: ${join(STAGING, REVISION)}`);
  process.exit(0);
}

for (const modelRoot of modelRoots()) {
  const candidate = join(modelRoot, 'nemotron-diarization-runtime');
  if (stageFrom(candidate)) {
    console.log(`Staged verified diarization runtime from ${candidate}`);
    process.exit(0);
  }
}

let source = nativeSource();
if (
  strict &&
  !process.env.NEMO_SPEECH_SOURCE &&
  (!source || source === join(root, '.tooling/nemo-speech.cpp'))
) {
  source = buildDiarizationRuntime(root, REVISION);
}
if (source) {
  // Package into a fresh directory so an invalid previous stage can be repaired
  // without replacing a developer's installed model/runtime bundle.
  const stagingParent = join(root, '.tooling/diarization-runtime');
  mkdirSync(stagingParent, { recursive: true });
  const temporary = mkdtempSync(join(stagingParent, '.package-'));
  try {
    execFileSync(
      'uv',
      [
        'run',
        '--locked',
        'python',
        join(root, 'scripts/package-nemotron-diarization.py'),
        '--source',
        source,
        '--model-root',
        temporary,
      ],
      { cwd: root, stdio: 'inherit' },
    );
    if (!stageFrom(join(temporary, 'nemotron-diarization-runtime')))
      throw new Error('Packaged diarization runtime failed verification');
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
  console.log(`Packaged diarization runtime: ${join(STAGING, REVISION)}`);
  process.exit(0);
}

mkdirSync(STAGING, { recursive: true });
const message = `No diarization runtime source: set NEMO_SPEECH_SOURCE to the pinned NeMo-Speech.cpp checkout, or keep a verified bundle in a model directory. Staging left empty; speaker timelines stay unavailable.`;
if (strict) throw new Error(message);
console.warn(`warning: ${message}`);
