#!/usr/bin/env node
// Explicit macOS/public-audio experiment. Build first; the frontend server is separate.
import { createHash } from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import {
  createReadStream,
  closeSync,
  mkdirSync,
  openSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const [binaryArg, modelsArg, audioArg, outputArg] = process.argv.slice(2);
if (process.platform !== 'darwin' || !binaryArg || !modelsArg || !audioArg || !outputArg) {
  throw Error(
    'macOS usage: run-native-learning.mjs BINARY MODEL_ROOT PUBLIC_AUDIO_ROOT NEW_OUTPUT_DIRECTORY',
  );
}
const cwd = fileURLToPath(new URL('../', import.meta.url));
const binary = resolve(binaryArg),
  models = resolve(modelsArg),
  audio = resolve(audioArg),
  directory = resolve(outputArg);
const run = (command, args) => execFileSync(command, args, { cwd, encoding: 'utf8' }).trim();
mkdirSync(directory); // Never overwrite a previous run.
const digest = createHash('sha256');
for await (const chunk of createReadStream(binary)) digest.update(chunk);
const sources = run('git', [
  'ls-files',
  'apps/desktop/src',
  'apps/desktop/src-tauri/src',
  'crates',
  'Cargo.lock',
])
  .split('\n')
  .filter((p) => !p.includes('/tests/fixtures/'));
sources.push(
  'apps/desktop/src-tauri/examples/learning_webview.rs',
  'apps/desktop/tests/native-learning.html',
  'apps/desktop/tests/native-learning.mjs',
  'scripts/run-native-learning.mjs',
  'scripts/summarize-native-learning.mjs',
  'benchmarks/asr/manifests/native-learning-2026-09-19.json',
);
const identity = {
  revision: run('git', ['rev-parse', 'HEAD']),
  binarySha256: digest.digest('hex'),
  sources: {},
};
for (const file of sources)
  identity.sources[file] = createHash('sha256')
    .update(readFileSync(resolve(cwd, file)))
    .digest('hex');
writeFileSync(resolve(directory, 'source-identity.json'), `${JSON.stringify(identity, null, 2)}\n`);
const environment = {
  startedAt: new Date().toISOString(),
  os: run('sw_vers', []),
  hardware: run('sysctl', ['-n', 'hw.model', 'hw.memsize', 'hw.logicalcpu']),
  initialPower: run('pmset', ['-g', 'batt']),
  build: 'Use source identity and supplied executable; record its build mode in the evaluation',
  samples: [],
};
const persist = () =>
  writeFileSync(
    resolve(directory, 'environment.json'),
    `${JSON.stringify(environment, null, 2)}\n`,
  );
persist();
const fd = openSync(resolve(directory, 'native.log'), 'wx');
const child = spawn('/usr/bin/time', ['-l', binary], {
  cwd,
  detached: true,
  stdio: ['ignore', fd, fd],
  env: {
    ...process.env,
    VESPER_MODEL_DIR: models,
    VESPER_LEARNING_AUDIO_ROOT: audio,
    VESPER_WEBVIEW_REPORT: resolve(directory, 'requests.jsonl'),
  },
});
console.log(`Native qualification artifacts: ${directory}`);
const monitor = setInterval(() => {
  try {
    environment.samples.push({
      at: new Date().toISOString(),
      thermal: run('pmset', ['-g', 'therm']),
    });
  } catch (error) {
    environment.samples.push({ at: new Date().toISOString(), error: error.message });
  }
  persist();
}, 10_000);
const timeout = setTimeout(() => {
  environment.timedOut = true;
  if (child.pid) process.kill(-child.pid, 'SIGTERM');
}, 22 * 60_000);
child.on('error', (error) => {
  environment.launchError = error.message;
  persist();
});
child.on('close', (code, signal) => {
  clearInterval(monitor);
  clearTimeout(timeout);
  closeSync(fd);
  environment.finishedAt = new Date().toISOString();
  environment.exitCode = code;
  environment.signal = signal;
  try {
    environment.finalPower = run('pmset', ['-g', 'batt']);
  } catch (error) {
    environment.finalPowerError = error.message;
  }
  persist();
  console.log(JSON.stringify({ code, signal }));
  process.exitCode = code ?? 1;
});
