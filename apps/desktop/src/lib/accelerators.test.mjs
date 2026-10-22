import assert from 'node:assert/strict';
import test from 'node:test';
import { defaultSettings } from './config.ts';
import { acceleratorOptions, acceleratorReason, deviceLabel } from './accelerators.ts';

const device = (os, arch, ...backends) => ({
  os,
  arch,
  backends: backends.map((backend) => ({
    backend,
    compiled: true,
    devices: [`Synthetic ${backend} GPU`],
  })),
});
const enabled = (config, caps) =>
  acceleratorOptions(config, caps)
    .filter((option) => !option.disabled)
    .map((option) => option.value);

test('accelerators follow operating system, detected GPU and app build', () => {
  const config = defaultSettings();
  config.models.asr = 'nvidia/nemotron-3.5-asr-streaming-0.6b';
  assert.deepEqual(enabled(config, device('macos', 'aarch64', 'metal', 'cuda')), [
    'auto',
    'cpu',
    'metal',
  ]);
  assert.deepEqual(enabled(config, device('macos', 'x86_64', 'metal')), ['auto', 'cpu']);
  assert.deepEqual(enabled(config, device('windows', 'x86_64', 'cuda')), ['auto', 'cpu', 'cuda']);
  assert.deepEqual(enabled(config, device('linux', 'x86_64', 'vulkan', 'rocm')), [
    'auto',
    'cpu',
    'vulkan',
    'rocm',
  ]);
  assert.deepEqual(enabled(config, device('linux', 'aarch64', 'vulkan', 'rocm')), [
    'auto',
    'cpu',
    'vulkan',
  ]);
  const amdCpu = {
    os: 'linux',
    arch: 'x86_64',
    backends: [{ backend: 'rocm', compiled: true, devices: [] }],
  };
  assert.deepEqual(enabled(config, amdCpu), ['auto', 'cpu']);
  const absentBuild = device('windows', 'x86_64', 'cuda');
  absentBuild.backends[0].compiled = false;
  assert.deepEqual(enabled(config, absentBuild), ['auto', 'cpu']);
  assert.match(deviceLabel(device('macos', 'aarch64')), /macOS · Apple Silicon/);
});

test('model restrictions and cleanup off narrow GPU choices without changing saved settings', () => {
  const caps = device('macos', 'aarch64', 'metal');
  const config = defaultSettings();
  config.models.asrEngine = 'standard'; // these cases cover the standard ONNX/MLX engine
  config.computeBackend = 'metal';
  for (const model of [
    'nvidia/parakeet-tdt-0.6b-v3',
    'Qwen/Qwen3-ASR-0.6B',
    'CohereLabs/cohere-transcribe-03-2026',
  ]) {
    config.models.asr = model;
    assert.deepEqual(enabled(config, caps), ['auto', 'cpu']);
    assert.equal(config.computeBackend, 'metal');
    assert.equal(acceleratorOptions(config, caps).at(-1).disabled, true);
  }
  config.models.asr = 'LiquidAI/LFM2.5-Audio-1.5B';
  assert.deepEqual(enabled(config, caps), ['auto', 'cpu', 'metal']);
  config.models.customAsr = { id: 'synthetic' };
  config.cleanupEnabled = false;
  assert.deepEqual(enabled(config, caps), ['auto', 'cpu']);
  config.cleanupEnabled = true;
  assert.deepEqual(enabled(config, caps), ['auto', 'cpu', 'metal']);
});

test('unknown detection keeps safe choices and explains unavailable saved hardware', () => {
  const config = defaultSettings();
  config.computeBackend = 'cuda';
  assert.deepEqual(enabled(config, null), ['auto', 'cpu']);
  assert.match(acceleratorOptions(config, null).at(-1).label, /not checked/);
  assert.match(
    acceleratorReason(config, device('macos', 'aarch64', 'metal'), 'cuda'),
    /operating system/,
  );
  assert.equal(config.computeBackend, 'cuda');
});

test('MLX is explicit, packaged, English-only and model-qualified', () => {
  const caps = {
    ...device('macos', 'aarch64', 'metal'),
    speechMetalModels: ['nvidia/parakeet-tdt-0.6b-v3', 'Qwen/Qwen3-ASR-0.6B'],
  };
  const config = defaultSettings();
  config.models.asrEngine = 'standard'; // these cases cover the standard ONNX/MLX engine
  for (const model of caps.speechMetalModels) {
    config.models.asr = model;
    config.language = 'en';
    assert.deepEqual(enabled(config, caps), ['auto', 'cpu', 'metal']);
    for (const language of ['auto', 'fr', 'hi']) {
      config.language = language;
      assert.deepEqual(enabled(config, caps), ['auto', 'cpu']);
    }
  }
  config.language = 'en';
  config.models.asr = 'CohereLabs/cohere-transcribe-03-2026';
  assert.deepEqual(enabled(config, caps), ['auto', 'cpu']);
});

test('Apple-GPU-only speech models offer Automatic and Metal for every declared language', () => {
  const granite = 'ibm-granite/granite-speech-5.0-470m-turboctc';
  const config = defaultSettings();
  config.models.asr = granite;
  const packaged = { ...device('macos', 'aarch64', 'metal'), speechMetalModels: [granite] };
  for (const language of ['en', 'de', 'ja']) {
    config.language = language;
    assert.deepEqual(enabled(config, packaged), ['auto', 'metal']);
  }
  assert.match(acceleratorReason(config, packaged, 'cpu'), /only on the Apple GPU/);
  const cpu = acceleratorOptions(config, packaged).find((option) => option.value === 'cpu');
  assert.equal(cpu.label, 'CPU · unavailable');
  assert.equal(acceleratorOptions(config, packaged)[0].description, 'Use the Apple GPU');
  const unpackaged = { ...device('macos', 'aarch64', 'metal'), speechMetalModels: [] };
  assert.deepEqual(enabled(config, unpackaged), ['auto']);
  assert.deepEqual(enabled(config, device('linux', 'x86_64', 'cuda')), ['auto']);
});

test('Phonon-2 is English-only and offered only on a packaged Apple GPU', () => {
  const phonon = 'FermionResearch/Phonon-2';
  const config = defaultSettings();
  config.models.asr = phonon;
  config.language = 'en';
  const packaged = { ...device('macos', 'aarch64', 'metal'), speechMetalModels: [phonon] };
  assert.deepEqual(enabled(config, packaged), ['auto', 'metal']);
  assert.deepEqual(enabled(config, device('windows', 'x86_64', 'cuda')), ['auto']);
});

test('Parakeet Redux offers Automatic and Metal for automatic detection and its languages', () => {
  const redux = 'moondream/parakeet-redux';
  const config = defaultSettings();
  config.models.asr = redux;
  const packaged = { ...device('macos', 'aarch64', 'metal'), speechMetalModels: [redux] };
  for (const language of ['auto', 'en', 'de']) {
    config.language = language;
    assert.deepEqual(enabled(config, packaged), ['auto', 'metal']);
  }
});

test('the unified Parakeet GGUF offers every NeMo device, the standard engine stays CPU or English Metal', () => {
  const config = defaultSettings();
  config.models.asr = 'nvidia/parakeet-tdt-0.6b-v3';
  config.models.asrEngine = 'standard';
  config.language = 'de';
  const mac = { ...device('macos', 'aarch64', 'metal'), speechMetalModels: [] };
  assert.deepEqual(enabled(config, device('windows', 'x86_64', 'cuda', 'vulkan')), ['auto', 'cpu']);
  assert.deepEqual(enabled(config, mac), ['auto', 'cpu']);
  // Automatic (the default) and the explicit override both pick the GGUF.
  config.models.asrEngine = 'automatic';
  assert.deepEqual(enabled(config, device('windows', 'x86_64', 'cuda', 'vulkan')), [
    'auto',
    'cpu',
    'cuda',
    'vulkan',
  ]);
  assert.deepEqual(enabled(config, mac), ['auto', 'cpu', 'metal']);
  // Another model ignores a leftover engine setting.
  config.models.asr = 'Qwen/Qwen3-ASR-0.6B';
  assert.deepEqual(enabled(config, device('windows', 'x86_64', 'cuda')), ['auto', 'cpu']);
});
