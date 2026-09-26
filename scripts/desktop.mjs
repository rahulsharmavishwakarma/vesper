#!/usr/bin/env node
// One target-aware entry point for development, debug bundles and releases.
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export const mlxOverride = (environment = process.env) =>
  environment.VESPER_MLX ?? environment.TRANSCRIBE_MLX;

export function desktopPlan(
  args,
  { platform, arch, host, environmentTarget, configuredTarget, mlx = 'auto' },
) {
  const separator = args.indexOf('--');
  const head = separator < 0 ? [...args] : args.slice(0, separator);
  const tail = separator < 0 ? [] : args.slice(separator);
  if (head.some((arg) => ['--help', '-h', '--version', '-V'].includes(arg)))
    return { args, prepareMlx: false };
  if (!['dev', 'build'].includes(head[0])) throw new Error('Expected desktop dev or build');
  if (
    tail
      .slice(1, tail.indexOf('--', 1) < 0 ? undefined : tail.indexOf('--', 1))
      .some((arg) => arg === '--target' || arg.startsWith('--target='))
  )
    throw new Error('Pass --target before the Cargo separator so Tauri and resources agree.');
  const targets = head.flatMap((arg, index) =>
    arg === '--target' || arg === '-t'
      ? [head[index + 1]]
      : arg.startsWith('--target=')
        ? [arg.slice(9)]
        : arg.startsWith('-t') && !arg.startsWith('--')
          ? [arg.slice(2)]
          : [],
  );
  if (targets.length > 1 || targets.some((target) => !target || target.startsWith('-')))
    throw new Error('Expected a single valid Tauri --target');
  if (!targets.length && !environmentTarget && configuredTarget)
    throw new Error('Custom Cargo target configuration: pass an explicit Tauri --target.');
  const target = targets[0] || environmentTarget || host;
  if (!targets.length && environmentTarget) head.push('--target', environmentTarget);
  if (!['auto', '0'].includes(mlx)) throw new Error('VESPER_MLX must be auto or 0');
  const prepareMlx =
    mlx !== '0' && platform === 'darwin' && arch === 'arm64' && target === 'aarch64-apple-darwin';
  if (prepareMlx) head.push('--config', 'src-tauri/tauri.mlx.conf.json');
  if (head[0] === 'build' && !tail.includes('--locked')) {
    if (!tail.length) tail.push('--');
    tail.push('--locked');
  }
  return { args: [...head, ...tail], prepareMlx };
}

function hasConfiguredTarget() {
  const directories = new Set([process.env.CARGO_HOME || join(homedir(), '.cargo')]);
  for (let path = join(root, 'apps/desktop/src-tauri'); ; path = dirname(path)) {
    directories.add(join(path, '.cargo'));
    if (dirname(path) === path) break;
  }
  // Do not implement a second TOML parser. Any target setting in a Cargo file
  // requires an explicit CLI target, including quoted/dotted or included config.
  return [...directories].some((path) =>
    ['config', 'config.toml'].some((name) => {
      const file = join(path, name);
      return (
        existsSync(file) && /\b(?:target|include)\b\s*["']?\s*=/.test(readFileSync(file, 'utf8'))
      );
    }),
  );
}

function main() {
  const args = process.argv.slice(2);
  const help = args.some((arg) => ['--help', '-h', '--version', '-V'].includes(arg));
  const host = help
    ? ''
    : execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)?.[1];
  const plan = desktopPlan(args, {
    platform: process.platform,
    arch: process.arch,
    host,
    environmentTarget: process.env.CARGO_BUILD_TARGET,
    configuredTarget: !help && hasConfiguredTarget(),
    mlx: mlxOverride(),
  });
  if (plan.prepareMlx) {
    const version = execFileSync('sw_vers', ['-productVersion'], { encoding: 'utf8' });
    if (Number(version.split('.')[0]) < 14)
      throw new Error('MLX development requires macOS 14 or newer.');
    execFileSync(process.execPath, [join(root, 'scripts/mlx-runtime.mjs')], { stdio: 'inherit' });
  }
  const result = spawnSync(
    process.execPath,
    [join(root, 'node_modules/@tauri-apps/cli/tauri.js'), ...plan.args],
    { cwd: join(root, 'apps/desktop'), stdio: 'inherit' },
  );
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
