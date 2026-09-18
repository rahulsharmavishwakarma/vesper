#!/usr/bin/env node
// Explicit native build; model weights are never downloaded here.
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = fileURLToPath(new URL('../', import.meta.url));
const revision = '11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf';
const sourceSha256 = '0a8db6c55dd318f4a688faba85f7760b99a6c92e8ef8864479d418531bee1ac2';
const patch = join(root, 'patches/sherpa-onnx/decode-report.patch');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const patchSha256 = hash(readFileSync(patch));
const cache = join(root, '.tooling/sherpa-runtime');
const current = join(cache, 'current.json');
const libs = [
  'sherpa-onnx-c-api',
  'sherpa-onnx-core',
  'kaldi-decoder-core',
  'sherpa-onnx-kaldifst-core',
  'sherpa-onnx-fstfar',
  'sherpa-onnx-fst',
  'kaldi-native-fbank-core',
  'kissfft-float',
  'piper_phonemize',
  'espeak-ng',
  'ucd',
  'onnxruntime',
  'ssentencepiece_core',
];
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: root, stdio: ['ignore', 2, 2], ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with ${result.status}`);
  return result.stdout?.toString().trim();
}
function installed() {
  if (!existsSync(current)) return null;
  const manifest = JSON.parse(readFileSync(current, 'utf8'));
  if (
    manifest.patchSha256 !== patchSha256 ||
    manifest.revision !== revision ||
    manifest.platform !== process.platform ||
    manifest.arch !== process.arch ||
    !libs.every((name) => existsSync(join(manifest.libDir, `lib${name}.a`)))
  )
    return null;
  return manifest;
}

const mode = process.argv[2] ?? '--build';
if (mode === '--lib-dir' || mode === '--manifest' || mode === '--source-dir') {
  const manifest = installed();
  if (manifest)
    process.stdout.write(
      mode === '--source-dir'
        ? resolve(manifest.libDir, '../../source')
        : mode === '--manifest'
          ? join(manifest.libDir, 'transcribe-runtime.json')
          : manifest.libDir,
    );
  process.exit(0);
}
if (mode !== '--build')
  throw new Error('Usage: node scripts/sherpa-runtime.mjs [--build|--lib-dir|--manifest]');
if (!['darwin', 'linux'].includes(process.platform) || !['arm64', 'x64'].includes(process.arch)) {
  throw new Error(
    'The patched runtime build currently targets macOS/Linux arm64/x64. Other targets require native qualification.',
  );
}
mkdirSync(cache, { recursive: true });
const compiler = run(process.env.CXX || 'c++', ['--version'], { stdio: ['ignore', 'pipe', 2] });
const cmake = run('cmake', ['--version'], { stdio: ['ignore', 'pipe', 2] });
const flags = [
  '-DCMAKE_BUILD_TYPE=Release',
  '-DBUILD_SHARED_LIBS=OFF',
  '-DSHERPA_ONNX_ENABLE_C_API=ON',
  '-DSHERPA_ONNX_ENABLE_BINARY=OFF',
  '-DSHERPA_ONNX_BUILD_C_API_EXAMPLES=OFF',
  '-DSHERPA_ONNX_ENABLE_PYTHON=OFF',
  '-DSHERPA_ONNX_ENABLE_JNI=OFF',
  '-DSHERPA_ONNX_ENABLE_WEBSOCKET=OFF',
  '-DSHERPA_ONNX_ENABLE_PORTAUDIO=OFF',
  '-DSHERPA_ONNX_ENABLE_GPU=OFF',
  '-DSHERPA_ONNX_USE_PRE_INSTALLED_ONNXRUNTIME_IF_AVAILABLE=OFF',
];
if (process.platform === 'darwin')
  flags.push(`-DCMAKE_OSX_ARCHITECTURES=${process.arch === 'arm64' ? 'arm64' : 'x86_64'}`);
const identity = {
  revision,
  sourceSha256,
  patchSha256,
  platform: process.platform,
  arch: process.arch,
  compiler,
  cmake,
  flags,
};
const buildId = hash(JSON.stringify(identity));
const work = join(cache, buildId);
const source = join(work, 'source');
const build = join(work, 'build');
const install = join(work, 'install');
const libDir = join(install, 'lib');
const manifestPath = join(libDir, 'transcribe-runtime.json');
if (existsSync(manifestPath)) {
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (
    manifest.buildId === buildId &&
    libs.every((name) => {
      const filename = `lib${name}.a`;
      return (
        existsSync(join(libDir, filename)) &&
        hash(readFileSync(join(libDir, filename))) === manifest.libraries[filename]
      );
    })
  ) {
    writeFileSync(current, JSON.stringify(manifest, null, 2) + '\n');
    console.error(`Verified patched sherpa-onnx ${buildId.slice(0, 12)}`);
    process.exit(0);
  }
  throw new Error('Cached native library integrity failed. Remove that build cache and rebuild.');
}
const archive = join(cache, `${revision}.tar.gz`);
if (!existsSync(archive))
  run('curl', [
    '--fail',
    '--location',
    '--retry',
    '3',
    `https://codeload.github.com/k2-fsa/sherpa-onnx/tar.gz/${revision}`,
    '--output',
    archive,
  ]);
if (hash(readFileSync(archive)) !== sourceSha256)
  throw new Error('Sherpa source archive checksum mismatch');
mkdirSync(source, { recursive: true });
if (!existsSync(join(source, '.transcribe-patched'))) {
  run('tar', ['-xzf', archive, '--strip-components=1', '-C', source]);
  run('patch', ['--batch', '-p1', '-i', patch], { cwd: source });
  writeFileSync(join(source, '.transcribe-patched'), patchSha256);
}
run('cmake', ['-S', source, '-B', build, `-DCMAKE_INSTALL_PREFIX=${install}`, ...flags]);
run('cmake', [
  '--build',
  build,
  '--config',
  'Release',
  '--parallel',
  process.env.CMAKE_BUILD_PARALLEL_LEVEL || '4',
]);
run('cmake', ['--install', build, '--config', 'Release']);
const libraries = Object.fromEntries(
  libs.map((name) => {
    const filename = `lib${name}.a`;
    return [filename, hash(readFileSync(join(libDir, filename)))];
  }),
);
const manifest = { ...identity, buildId, libDir: resolve(libDir), libraries };
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
writeFileSync(current, JSON.stringify(manifest, null, 2) + '\n');
console.error(
  `Built patched sherpa-onnx ${buildId.slice(0, 12)}; source scripts/dev-env.sh before cargo commands.`,
);
