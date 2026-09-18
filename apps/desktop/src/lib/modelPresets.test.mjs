import assert from 'node:assert/strict';
import test from 'node:test';
import { defaultSettings, parseStoredSettings, validateSettings } from './config.ts';
import { downloadTarget } from './modelDownloads.ts';
import {
  MODEL_PRESETS,
  modelOptions,
  presetForProfile,
  selectModel,
  selectedModelValue,
} from './modelPresets.ts';

test('catalog has distinct pinned profiles for both pipeline stages', () => {
  assert.ok(MODEL_PRESETS.some((entry) => entry.profile.runtime === 'sherpa_onnx_transducer'));
  assert.ok(MODEL_PRESETS.some((entry) => entry.profile.runtime === 'llama_cpp'));
  assert.equal(new Set(MODEL_PRESETS.map((entry) => entry.profile.id)).size, MODEL_PRESETS.length);
  for (const entry of MODEL_PRESETS) {
    assert.ok(entry.description && entry.precision);
    assert.ok(
      ['huggingface.co', 'catalog.ngc.nvidia.com'].includes(new URL(entry.sourceUrl).hostname),
    );
  }
});

test('every preset saves, reloads and resolves to its exact download profile', () => {
  for (const { profile } of MODEL_PRESETS) {
    const role = profile.runtime === 'llama_cpp' ? 'rewriter' : 'asr';
    const config = defaultSettings();
    config.rewriteQuantization = 'q8_0';
    config.models = selectModel(config.models, role, `preset:${profile.id}`);
    const restored = parseStoredSettings(JSON.parse(JSON.stringify(config)));
    assert.deepEqual(validateSettings(restored), []);
    assert.equal(selectedModelValue(restored.models, role), `preset:${profile.id}`);
    assert.equal(
      modelOptions(restored.models, role).filter(
        (option) => option.value === `preset:${profile.id}`,
      ).length,
      1,
    );
    assert.equal(
      modelOptions(restored.models, role).some((option) => option.value === 'custom'),
      false,
    );
    assert.deepEqual(downloadTarget(restored, role), {
      modelId: profile.id,
      name: profile.name,
      quantization: null,
      role: role === 'asr' ? 'speech_recognition' : 'text_reconstruction',
    });
  }
});

test('switching a stage preserves the other stage and never mutates the catalog', () => {
  const before = JSON.stringify(MODEL_PRESETS);
  const speech = MODEL_PRESETS.find(
    (entry) => entry.profile.runtime === 'sherpa_onnx_transducer',
  ).profile;
  const cleanup = MODEL_PRESETS.find((entry) => entry.profile.runtime === 'llama_cpp').profile;
  let models = selectModel(defaultSettings().models, 'asr', `preset:${speech.id}`);
  models = selectModel(models, 'rewriter', `preset:${cleanup.id}`);
  models.customRewriter.name = 'User edit';
  assert.equal(JSON.stringify(MODEL_PRESETS), before);
  models = selectModel(models, 'asr', defaultSettings().models.asr);
  assert.equal(models.customAsr, null);
  assert.equal(models.customRewriter.name, 'User edit');
  assert.throws(() => selectModel(models, 'asr', `preset:${cleanup.id}`));
});

test('an edited imported profile cannot masquerade as a preset with the same ID', () => {
  const preset = MODEL_PRESETS.find((entry) => entry.profile.runtime === 'llama_cpp');
  const profile = structuredClone(preset.profile);
  profile.files[0].sha256 = 'a'.repeat(64);
  const models = { ...defaultSettings().models, customRewriter: profile };
  assert.equal(presetForProfile(profile), undefined);
  assert.equal(selectedModelValue(models, 'rewriter'), 'custom');
  assert.equal(
    modelOptions(models, 'rewriter').filter((option) => option.value === 'custom').length,
    1,
  );
  assert.equal(
    modelOptions(models, 'rewriter').find((option) => option.value === 'custom').creator,
    'Creator not specified',
  );
  const normalized = {
    ...preset.profile,
    modelType: null,
    languages: preset.profile.languages ?? [],
    files: [...preset.profile.files].reverse(),
  };
  assert.equal(presetForProfile(normalized), preset);
});

test('creator identity follows original catalog provenance, not community packaging', () => {
  const models = defaultSettings().models;
  const speech = modelOptions(models, 'asr');
  const writing = modelOptions(models, 'rewriter');
  assert.equal(
    speech.find((option) => option.value === 'preset:parakeet-tdt-0.6b-v2-int8').creator,
    'NVIDIA',
  );
  assert.equal(writing.find((option) => option.label.startsWith('Gemma')).creator, 'Google');
  assert.equal(
    writing.find((option) => option.label.startsWith('SmolLM2')).creator,
    'Hugging Face',
  );
  assert.equal(
    writing.find((option) => option.value === models.rewriter).creator,
    'Qwen · Alibaba',
  );
  assert.ok(
    [...speech, ...writing].every(
      (option) => option.creator && option.creator !== 'Creator not specified',
    ),
  );
  const profile = structuredClone(
    MODEL_PRESETS.find((entry) => entry.profile.runtime === 'llama_cpp').profile,
  );
  profile.name = 'NVIDIA official model';
  profile.repository = 'nvidia/imported-model';
  const custom = modelOptions({ ...models, customRewriter: profile }, 'rewriter').find(
    (option) => option.value === 'custom',
  );
  assert.equal(
    custom.creator,
    'Creator not specified',
    'An imported namespace is not proof of original creator identity',
  );
});

test('speech preset language restrictions survive selection and validation', () => {
  const english = MODEL_PRESETS.find(
    (entry) =>
      entry.profile.runtime === 'sherpa_onnx_transducer' && !entry.profile.languages.includes('hi'),
  );
  const settings = defaultSettings();
  settings.language = 'hi';
  settings.models = selectModel(settings.models, 'asr', `preset:${english.profile.id}`);
  assert.ok(validateSettings(settings).some((error) => error.includes('language')));
});

test('dedicated ASR choices preserve settings, enforce languages and ignore obsolete duration quotas', async () => {
  const { effectiveRecordingSeconds } = await import('./config.ts');
  for (const id of ['Qwen/Qwen3-ASR-0.6B', 'CohereLabs/cohere-transcribe-03-2026']) {
    for (const language of ['auto', 'en', 'fr', 'hi']) {
      const settings = defaultSettings();
      settings.models = selectModel(settings.models, 'asr', id);
      settings.language = language;
      const valid = id.startsWith('Qwen/') || ['en', 'fr'].includes(language);
      assert.equal(validateSettings(settings).length === 0, valid);
      assert.equal(effectiveRecordingSeconds(settings), 0);
      assert.equal(settings.maxRecordingSeconds, 0);
      assert.equal(downloadTarget(settings, 'asr').modelId, id);
      assert.equal(downloadTarget(settings, 'asr').quantization, null);
      if (valid)
        assert.deepEqual(parseStoredSettings(JSON.parse(JSON.stringify(settings))), settings);
      settings.computeBackend = 'metal';
      assert.ok(validateSettings(settings).length);
      settings.maxRecordingSeconds = 5;
      assert.equal(effectiveRecordingSeconds(settings), 0);
    }
  }
});
