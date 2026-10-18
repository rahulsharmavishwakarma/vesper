#!/usr/bin/env node
// Pinned optional application runtime. No model downloads or app-default changes.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { prepareMlxBuildDirectories } from './mlx-build-directories.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const run = (command, args, options = {}) =>
  execFileSync(command, args, { cwd: root, stdio: 'inherit', ...options });
if (process.platform !== 'darwin' || process.arch !== 'arm64')
  throw new Error('This bridge build targets Apple Silicon macOS.');
if (process.argv.length > 2) throw new Error('usage: node scripts/mlx-runtime.mjs');
const { scratch, cache, destination } = prepareMlxBuildDirectories(root);
const args = [
  '--package-path',
  join(root, 'native/mlx'),
  '--scratch-path',
  scratch,
  '--cache-path',
  cache,
];
run('xcrun', ['metal', '--version']);
run('swift', ['package', ...args, 'resolve', '--force-resolved-versions']);
const checkout = join(scratch, 'checkouts/mlx-audio-swift');
const revision = run('git', ['rev-parse', 'HEAD'], {
  cwd: checkout,
  stdio: 'pipe',
  encoding: 'utf8',
}).trim();
if (revision !== 'd302a5c6080d2bb97bae38c7418f82abb76013b6')
  throw new Error('Unexpected MLX Audio revision');
const patches = ['completion-report.patch', 'live-session.patch'].map((name) =>
  join(root, 'patches/mlx-audio-swift', name),
);
for (const patch of patches) {
  try {
    run('git', ['apply', '--reverse', '--check', patch], { cwd: checkout, stdio: 'pipe' });
  } catch {
    run('git', ['apply', '--check', patch], { cwd: checkout });
    run('git', ['apply', patch], { cwd: checkout });
  }
}
// A reverse-apply check alone also accepts unrelated edits. Reconstruct the
// expected patched files outside the checkout and compare their exact bytes.
const patchedPaths = [
  ...new Set(
    patches.flatMap((patch) =>
      [...readFileSync(patch, 'utf8').matchAll(/^\+\+\+ b\/(.+)$/gm)].map((match) => match[1]),
    ),
  ),
];
const changed = run('git', ['diff', '--name-only', 'HEAD'], {
  cwd: checkout,
  stdio: 'pipe',
  encoding: 'utf8',
})
  .trim()
  .split('\n')
  .filter(Boolean);
if (changed.some((path) => !patchedPaths.includes(path)))
  throw new Error('Unexpected modified MLX Audio source');
const expected = mkdtempSync(join(tmpdir(), 'vesper-mlx-source-'));
try {
  for (const path of patchedPaths) {
    mkdirSync(dirname(join(expected, path)), { recursive: true });
    writeFileSync(
      join(expected, path),
      run('git', ['show', `HEAD:${path}`], {
        cwd: checkout,
        stdio: 'pipe',
      }),
    );
  }
  for (const patch of patches) run('git', ['apply', patch], { cwd: expected });
  for (const path of patchedPaths)
    if (!readFileSync(join(expected, path)).equals(readFileSync(join(checkout, path))))
      throw new Error(`Unexpected MLX Audio patch drift: ${path}`);
} finally {
  rmSync(expected, { recursive: true, force: true });
}
run('swift', [
  'build',
  ...args,
  '--force-resolved-versions',
  '-c',
  'release',
  '--product',
  'VesperMLX',
]);
run('swift', [
  'build',
  ...args,
  '--force-resolved-versions',
  '-c',
  'release',
  '--product',
  'prepare-qwen-tokenizer',
]);
const products = [join(scratch, 'out/Products/Release'), join(scratch, 'release')].find((path) =>
  existsSync(join(path, 'libVesperMLX.dylib')),
);
if (!products) throw new Error('Missing MLX library after successful build');
mkdirSync(destination, { recursive: true });
for (const name of readdirSync(products).filter(
  (name) =>
    name === 'libVesperMLX.dylib' || name === 'prepare-qwen-tokenizer' || name.endsWith('.bundle'),
))
  cpSync(join(products, name), join(destination, name), { recursive: true });
const metal = join(products, 'mlx-swift_Cmlx.bundle/Contents/Resources/default.metallib');
const flatMetal = join(products, 'mlx-swift_Cmlx.bundle/default.metallib');
// SwiftPM's native build system (the default before Swift 6.4, e.g. CI's Xcode
// 26) does not compile Metal shaders, as mlx-swift's README notes. Compile the
// package's precompiled-kernel sources the way MLX's CMake build does; the
// remaining kernels are JIT-compiled at runtime by MLX.
const compileMetalKernels = () => {
  const sources = join(scratch, 'checkouts/mlx-swift/Source/Cmlx/mlx-generated/metal');
  const kernels = execFileSync('find', [sources, '-name', '*.metal'], { encoding: 'utf8' })
    .trim()
    .split('\n')
    .filter(Boolean)
    .sort();
  if (!kernels.length) throw new Error('Missing MLX Metal kernel sources');
  const air = mkdtempSync(join(tmpdir(), 'vesper-mlx-air-'));
  try {
    const objects = kernels.map((source, index) => {
      const object = join(air, `${index}.air`);
      run('xcrun', [
        '-sdk',
        'macosx',
        'metal',
        '-x',
        'metal',
        '-Wall',
        '-Wextra',
        '-fno-fast-math',
        '-Wno-c++17-extensions',
        '-Wno-c++20-extensions',
        '-mmacosx-version-min=14.0',
        '-c',
        source,
        `-I${sources}`,
        '-o',
        object,
      ]);
      return object;
    });
    const library = join(air, 'default.metallib');
    run('xcrun', ['-sdk', 'macosx', 'metallib', ...objects, '-o', library]);
    const output = join(scratch, 'compiled-metal');
    mkdirSync(output, { recursive: true });
    cpSync(library, join(output, 'default.metallib'));
    console.log(`Compiled ${kernels.length} MLX Metal kernel sources outside SwiftPM.`);
    return join(output, 'default.metallib');
  } finally {
    rmSync(air, { recursive: true, force: true });
  }
};
const metalSource =
  (process.env.VESPER_MLX_COMPILE_METAL !== '1' && [metal, flatMetal].find(existsSync)) ||
  compileMetalKernels();
// MLX searches relative to its native image as well as SwiftPM bundles.
cpSync(metalSource, join(destination, 'mlx.metallib'));
const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
writeFileSync(
  join(destination, 'build-provenance.json'),
  JSON.stringify(
    {
      schemaVersion: 1,
      runtimeRevision: revision,
      packageLockSha256: hash(join(root, 'native/mlx/Package.resolved')),
      patchSha256: hash(patches[0]),
      livePatchSha256: hash(patches[1]),
      bridgeSha256: hash(join(root, 'native/mlx/Sources/VesperMLX/Bridge.swift')),
      liveBridgeSha256: hash(join(root, 'native/mlx/Sources/VesperMLX/Live.swift')),
      librarySha256: hash(join(destination, 'libVesperMLX.dylib')),
      metalSha256: hash(join(destination, 'mlx.metallib')),
      tokenizerToolSha256: hash(join(destination, 'prepare-qwen-tokenizer')),
      tokenizerSourceSha256: hash(join(root, 'native/mlx/Sources/PrepareTokenizer/main.swift')),
    },
    null,
    2,
  ) + '\n',
);
console.log(`Built optional application runtime: ${destination}`);
