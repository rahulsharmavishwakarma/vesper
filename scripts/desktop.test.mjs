import assert from 'node:assert/strict';
import { test } from 'node:test';
import { desktopPlan, mlxOverride } from './desktop.mjs';
import { execFileSync } from 'node:child_process';

test('MLX overrides prefer Vesper, preserve legacy fallback, and reject invalid current values', () => {
  assert.equal(mlxOverride({ VESPER_MLX: 'auto', TRANSCRIBE_MLX: '0' }), 'auto');
  assert.equal(mlxOverride({ TRANSCRIBE_MLX: '0' }), '0');
  assert.equal(mlxOverride({}), undefined);
  assert.throws(
    () =>
      desktopPlan(['build'], { ...mac, mlx: mlxOverride({ VESPER_MLX: '', TRANSCRIBE_MLX: '0' }) }),
    /VESPER_MLX/,
  );
});

test('development shell uses nonempty Vesper, Transcribe, Ukti, STT model overrides in order', () => {
  const names = ['VESPER_MODEL_DIR', 'TRANSCRIBE_MODEL_DIR', 'UKTI_MODEL_DIR', 'STT_MODEL_DIR'];
  for (let first = 0; first < names.length; first++) {
    const env = { ...process.env };
    names.forEach((name, index) => {
      env[name] = index < first ? '' : `/fixture/${index}`;
    });
    const result = execFileSync(
      '/bin/sh',
      ['-c', '. ./scripts/dev-env.sh; printf "%s" "$VESPER_MODEL_DIR"'],
      { env, encoding: 'utf8' },
    );
    assert.equal(result, `/fixture/${first}`);
  }
});

const mac = { platform: 'darwin', arch: 'arm64', host: 'aarch64-apple-darwin' };
test('default macOS builds need no updater key; signed and explicit configurations keep their policy', () => {
  const override = JSON.stringify({ bundle: { createUpdaterArtifacts: false } });
  const portable = desktopPlan(['build', '--', '--verbose'], {
    ...mac,
    mlx: '0',
    updaterSigningKey: false,
  });
  assert.deepEqual(portable.args, ['build', '--config', override, '--', '--verbose', '--locked']);
  const accelerated = desktopPlan(['build'], { ...mac, updaterSigningKey: false });
  assert.ok(accelerated.args.indexOf(override) < accelerated.args.indexOf('--'));
  for (const args of [
    ['build', '--config', 'custom.json'],
    ['build', '-ccustom.json'],
    ['build', '--config=custom.json'],
    ['dev'],
  ])
    assert.ok(!desktopPlan(args, { ...mac, updaterSigningKey: false }).args.includes(override));
  assert.ok(!desktopPlan(['build'], { ...mac, updaterSigningKey: true }).args.includes(override));
  // An enrolled public key without the private signing key still skips
  // updater artifacts; the app keeps the key to verify future updates.
  assert.ok(desktopPlan(['build'], { ...mac, updaterSigningKey: false }).args.includes(override));
  assert.ok(
    !desktopPlan(['build', '--target', 'aarch64-unknown-linux-gnu'], {
      ...mac,
      updaterSigningKey: false,
    }).args.includes(override),
  );
});
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
