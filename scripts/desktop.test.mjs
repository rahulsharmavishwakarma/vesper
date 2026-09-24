import assert from 'node:assert/strict';
import { test } from 'node:test';
import { desktopPlan } from './desktop.mjs';

const mac = { platform: 'darwin', arch: 'arm64', host: 'aarch64-apple-darwin' };
test('normal Apple Silicon dev/build adds MLX before Cargo arguments', () => {
  for (const mode of ['dev', 'build']) {
    const plan = desktopPlan([mode, '--', '--verbose'], mac);
    assert.equal(plan.prepareMlx, true);
    assert.deepEqual(plan.args.slice(0, 4), [
      mode,
      '--config',
      'src-tauri/tauri.mlx.conf.json',
      '--',
    ]);
    assert.equal(plan.args.includes('--locked'), mode === 'build');
  }
});
test('target overrides and unsupported hosts never accidentally package ARM MLX', () => {
  for (const target of [
    'x86_64-apple-darwin',
    'universal-apple-darwin',
    'aarch64-unknown-linux-gnu',
  ]) {
    assert.equal(desktopPlan(['build', '--target', target], mac).prepareMlx, false);
    assert.equal(desktopPlan(['build', `-t${target}`], mac).prepareMlx, false);
    const env = desktopPlan(['build'], { ...mac, environmentTarget: target });
    assert.equal(env.prepareMlx, false);
    assert.ok(env.args.includes(target));
  }
  for (const host of [{ platform: 'linux' }, { platform: 'win32' }, { arch: 'x64' }, { mlx: '0' }])
    assert.equal(desktopPlan(['build'], { ...mac, ...host }).prepareMlx, false);
  assert.equal(
    desktopPlan(['build', '--target=aarch64-apple-darwin'], {
      ...mac,
      environmentTarget: 'x86_64-apple-darwin',
    }).prepareMlx,
    true,
  );
});
test('help bypasses native tooling; ambiguous targets fail before building', () => {
  assert.deepEqual(desktopPlan(['build', '--help'], { configuredTarget: true }), {
    args: ['build', '--help'],
    prepareMlx: false,
  });
  for (const args of [
    ['build', '--', '--target', 'x86_64-apple-darwin'],
    ['build', '--target'],
    ['build', '--target=a', '--target=b'],
  ])
    assert.throws(() => desktopPlan(args, mac));
  assert.throws(() => desktopPlan(['build'], { ...mac, configuredTarget: true }));
  assert.equal(
    desktopPlan(['build', '--target=aarch64-apple-darwin'], { ...mac, configuredTarget: true })
      .prepareMlx,
    true,
  );
});
