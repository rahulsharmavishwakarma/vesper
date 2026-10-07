import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { OFFERED_MODEL_STATUSES } from './config.ts';
import { defaultSettings, parseStoredSettings, validateSettings } from './config.ts';
import { downloadTarget } from './modelDownloads.ts';
import {
  MODEL_LANGUAGE_FILTERS,
  MODEL_PRESETS,
  filterModelOptions,
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

test('reselecting a legacy writer adds declared languages without changing artifact identity', () => {
  for (const { profile } of MODEL_PRESETS.filter(
    (entry) => entry.profile.runtime === 'llama_cpp',
  )) {
    const config = defaultSettings();
    config.models.customRewriter = { ...structuredClone(profile), languages: [] };
    const loaded = parseStoredSettings(JSON.parse(JSON.stringify(config)));
    assert.equal(selectedModelValue(loaded.models, 'rewriter'), 'custom');
    assert.ok(validateSettings(loaded).some((error) => error.includes('no declared languages')));
    const repaired = {
      ...loaded,
      models: selectModel(loaded.models, 'rewriter', `preset:${profile.id}`),
    };
    assert.deepEqual(validateSettings(repaired), []);
    assert.deepEqual(repaired.models.customRewriter.files, loaded.models.customRewriter.files);
    assert.equal(repaired.models.customRewriter.revision, loaded.models.customRewriter.revision);
    assert.equal(repaired.models.customRewriter.id, loaded.models.customRewriter.id);
  }
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

test('picker options follow the model catalog, withheld entries stay out', () => {
  const catalog = JSON.parse(
    readFileSync(new URL('../../../../models/catalog.json', import.meta.url), 'utf8'),
  );
  const offered = OFFERED_MODEL_STATUSES;
  const speech = modelOptions(defaultSettings().models, 'asr');
  const writing = modelOptions(defaultSettings().models, 'rewriter');
  for (const entry of catalog.models) {
    const options = entry.role === 'speech_recognition' ? speech : writing;
    const found = options.find((option) => option.value === entry.id);
    if (offered.has(entry.status)) {
      assert.ok(found, `offered catalog model ${entry.id} must be selectable`);
      assert.equal(found.label, entry.name);
      assert.equal(found.creator, entry.creator);
      assert.equal(found.description, entry.picker_blurb);
    } else {
      assert.equal(found, undefined, `withheld catalog model ${entry.id} must stay out`);
    }
  }
});

test('picker options carry vendor-declared languages without runtime tags', () => {
  const manifest = JSON.parse(
    readFileSync(new URL('../../../../models/languages.json', import.meta.url), 'utf8'),
  );
  const models = defaultSettings().models;
  const speech = modelOptions(models, 'asr');
  const writing = modelOptions(models, 'rewriter');
  for (const option of [...speech, ...writing]) {
    assert.ok(Array.isArray(option.languages), `${option.value} must expose chip languages`);
    assert.equal(typeof option.unrestrictedLanguages, 'boolean');
    assert.ok(
      !/CPU|completed recordings/.test(option.description),
      `${option.value} blurb must not carry runtime tags`,
    );
    for (const language of option.languages) {
      assert.ok(language.value && language.label, 'every chip needs a code and a label');
    }
    assert.ok(
      !option.languages.some((language) => language.value === 'auto'),
      'auto is a mode, not a chip language',
    );
  }
  for (const [id, entry] of Object.entries(manifest)) {
    if (id.startsWith('_') || id === 'writers') continue;
    const found = speech.find((option) => option.value === id);
    if (!found) continue; // withheld models stay out of the picker
    assert.deepEqual(
      found.languages.map((language) => language.value),
      entry.languages,
      `chip mismatch for ${id}`,
    );
  }
  const writer = writing.find((option) => option.value === models.rewriter);
  assert.equal(writer.unrestrictedLanguages, true, 'built-in writer publishes no language list');
  assert.deepEqual(writer.languages, []);
});

test('picker language filter narrows rows to covering models', () => {
  assert.deepEqual(
    MODEL_LANGUAGE_FILTERS.map((filter) => filter.value),
    ['en', 'fr', 'hi'],
  );
  const models = defaultSettings().models;
  const speech = modelOptions(models, 'asr');
  const writing = modelOptions(models, 'rewriter');
  assert.deepEqual(
    filterModelOptions(speech, 'all').map((option) => option.value),
    speech.map((option) => option.value),
  );
  for (const option of filterModelOptions(speech, 'hi')) {
    assert.ok(
      option.languages.some((language) => language.value === 'hi'),
      `${option.value} must declare Hindi to survive the filter`,
    );
  }
  assert.ok(
    filterModelOptions(speech, 'hi').length < speech.length,
    'the Hindi filter must hide at least one speech row',
  );
  assert.ok(
    filterModelOptions(speech, 'fr').some((option) => option.value.includes('Cohere')),
    'Cohere must survive the French filter',
  );
  const writers = filterModelOptions(writing, 'hi');
  assert.ok(
    writers.some((option) => option.unrestrictedLanguages),
    'writers without a published list cover every filter language',
  );
  assert.deepEqual(filterModelOptions([], 'hi'), []);
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
      assert.equal(
        validateSettings(settings).length === 0,
        id.startsWith('Qwen/') && language === 'en',
      );
      settings.maxRecordingSeconds = 5;
      assert.equal(effectiveRecordingSeconds(settings), 0);
    }
  }
});
