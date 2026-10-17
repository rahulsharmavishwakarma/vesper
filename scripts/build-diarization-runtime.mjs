// Prepare the existing pinned Metal speaker runtime on a fresh build machine.
// Build dependencies stay project-local; no model weights are downloaded.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { DEPLOYMENT_TARGET } from './macho-minimum.mjs';

const sentencepieceRevision = '3863f7648e5d8edb571ac592f3ac4f5f0695275a'; // v0.1.99

export function buildDiarizationRuntime(
  root,
  revision,
  {
    platform = process.platform,
    arch = process.arch,
    environment = process.env,
    run = (command, args, options) => execFileSync(command, args, { stdio: 'inherit', ...options }),
  } = {},
) {
  if (platform !== 'darwin' || arch !== 'arm64')
    throw new Error('The diarization runtime build requires Apple silicon macOS');
  const source = join(root, '.tooling/nemo-speech.cpp');
  const dependency = join(root, '.tooling/diarization-build/sentencepiece');
  const dependencyBuild = join(root, '.tooling/diarization-build/sentencepiece-build');
  const parallel = environment.CMAKE_BUILD_PARALLEL_LEVEL || '4';
  if (!/^[1-9][0-9]*$/.test(parallel))
    throw new Error('CMAKE_BUILD_PARALLEL_LEVEL must be a positive integer');

  function checkout(path, repository, pinned, exclusions = []) {
    mkdirSync(path, { recursive: true });
    // Initialize/fetch without resetting an existing checkout or its local work.
    if (!existsSync(join(path, '.git'))) run('git', ['init', path]);
    const options = { cwd: path, stdio: ['ignore', 'pipe', 'inherit'], encoding: 'utf8' };
    let head;
    try {
      head = run('git', ['rev-parse', '--verify', '--quiet', 'HEAD'], options).trim();
    } catch {
      run('git', ['fetch', '--depth=1', repository, pinned], { cwd: path });
      run('git', ['checkout', '--detach', 'FETCH_HEAD'], { cwd: path });
      head = run('git', ['rev-parse', 'HEAD'], options).trim();
    }
    if (head !== pinned) throw new Error(`Unexpected source revision in ${path}`);
    if (run('git', ['diff', 'HEAD', '--', '.', ...exclusions], options).trim())
      throw new Error(
        `Native source has local changes in ${path}; preserve them before rebuilding`,
      );
  }

  console.log('Preparing pinned diarization runtime (first build requires network and CMake).');
  checkout(source, 'https://github.com/NVIDIA/NeMo-Speech.cpp.git', revision, [':(exclude)ggml']);
  if (!existsSync(join(source, 'ggml/CMakeLists.txt')))
    run('git', ['submodule', 'update', '--init', '--depth=1', 'ggml'], { cwd: source });
  checkout(dependency, 'https://github.com/google/sentencepiece.git', sentencepieceRevision);
  run('cmake', [
    '-S',
    dependency,
    '-B',
    dependencyBuild,
    '-DCMAKE_BUILD_TYPE=Release',
    '-DCMAKE_POLICY_VERSION_MINIMUM=3.5',
    '-DCMAKE_OSX_ARCHITECTURES=arm64',
    `-DCMAKE_OSX_DEPLOYMENT_TARGET=${DEPLOYMENT_TARGET}`,
    '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',
    '-DSPM_ENABLE_SHARED=OFF',
    '-DSPM_BUILD_TEST=OFF',
    '-DSPM_ENABLE_TCMALLOC=OFF',
  ]);
  run('cmake', [
    '--build',
    dependencyBuild,
    '--target',
    'sentencepiece-static',
    '--parallel',
    parallel,
  ]);
  run(
    'bash',
    [
      'scripts/configure.sh',
      'metal-diar',
      '-G',
      'Unix Makefiles',
      '-DNEMO_SPEECH_BUILD_MIC_CAPTURE=OFF',
      '-DCMAKE_OSX_ARCHITECTURES=arm64',
      `-DCMAKE_OSX_DEPLOYMENT_TARGET=${DEPLOYMENT_TARGET}`,
      // Portable CPU code: a native build targets the build machine's chip.
      '-DGGML_NATIVE=OFF',
      `-DSENTENCEPIECE_LIB=${join(dependencyBuild, 'src/libsentencepiece.a')}`,
      `-DSENTENCEPIECE_INCLUDE_DIR=${join(dependency, 'src')}`,
    ],
    { cwd: source },
  );
  run('cmake', ['--build', join(source, 'build/metal-diar'), '--parallel', parallel]);
  return source;
}
