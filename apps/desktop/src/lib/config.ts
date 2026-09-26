import { sameShortcut, shortcutIdentity } from './recording-shortcuts.ts';

export const ASR_MODELS = {
  'nvidia/parakeet-tdt-0.6b-v3': 'NVIDIA Parakeet TDT 0.6B v3',
  'nvidia/nemotron-3.5-asr-streaming-0.6b': 'NVIDIA Nemotron 3.5 ASR Streaming 0.6B',
  'Qwen/Qwen3-ASR-0.6B': 'Qwen3-ASR 0.6B',
  'CohereLabs/cohere-transcribe-03-2026': 'Cohere Transcribe',
  'LiquidAI/LFM2.5-Audio-1.5B': 'Liquid LFM2.5 Audio 1.5B',
} as const;

export type AsrModel = keyof typeof ASR_MODELS;
export type Language = 'auto' | 'en' | 'fr' | 'hi';
export type LanguageOption = { value: Language; label: string; description: string };
export type CleanupPreset = 'minimal' | 'balanced';
export type ComputeBackend = 'auto' | 'cpu' | 'metal' | 'cuda' | 'vulkan' | 'rocm';
export const AUDIO_FRONTENDS = {
  'silero-v6.2': 'Silero v6.2',
  dpdfnet8: 'DPDFNet8',
  'nvidia/Nemotron-3-Diarization': 'Nemotron 3 speaker timelines · 107 MB',
} as const;
export type AudioFrontendConfig = {
  vadEnabled: boolean;
  autoLevelEnabled: boolean;
  noiseReduction: 'off' | 'dpdfnet4' | 'dpdfnet8';
};
/** Fixed desktop profile. Legacy fields remain readable for settings migration. */
export function automaticAudioFrontend(): AudioFrontendConfig {
  return { vadEnabled: true, autoLevelEnabled: false, noiseReduction: 'dpdfnet8' };
}
export type ImprovementMode = 'review_required' | 'automatic';
export const CAPTURE_DESIGNS = [
  'split',
  'ribbon',
  'underline',
  'studio',
  'margin',
  'counter',
] as const;
export type CaptureDesign = (typeof CAPTURE_DESIGNS)[number];
export type ModelProfileRole = 'asr' | 'rewriter';
export const MODEL_ROLE_LABELS = { asr: 'Speech model', rewriter: 'Writing model' } as const;

/** Built-in cleanup models with pinned artifacts understood by transcribe-models. */
export const CLEANUP_MODELS = {
  'Qwen/Qwen3.5-0.8B': {
    name: 'Qwen3.5 0.8B',
    description: 'Small multilingual writing model that preserves transcript meaning',
    runtime: 'llama_cpp',
    quantizations: ['q4_0', 'q8_0'],
  },
} as const;

export type RewriteModel = keyof typeof CLEANUP_MODELS;

/** Packaged GGUF choices; sizes match the pinned artifacts manifest. */
export const CLEANUP_QUANTIZATIONS = {
  q4_0: {
    label: 'Q4 · Compact',
    description: 'Smaller model and lower memory use',
    sizeBytes: 563_036_064,
  },
  q8_0: {
    label: 'Q8 · Higher precision',
    description: 'Larger model with less quantization',
    sizeBytes: 833_592_096,
  },
} as const;

export type RewriteQuantization = keyof typeof CLEANUP_QUANTIZATIONS;

export const CLEANUP_MODEL_OPTIONS = (Object.keys(CLEANUP_MODELS) as RewriteModel[]).map(
  (value) => ({
    value,
    label: CLEANUP_MODELS[value].name,
    description: CLEANUP_MODELS[value].description,
  }),
);

export const CLEANUP_QUANTIZATION_OPTIONS = (
  Object.keys(CLEANUP_QUANTIZATIONS) as RewriteQuantization[]
).map((value) => ({
  value,
  label: CLEANUP_QUANTIZATIONS[value].label,
  description: CLEANUP_QUANTIZATIONS[value].description,
}));

export interface HuggingFaceProfile {
  id: string;
  name: string;
  repository: string;
  revision: string;
  runtime: 'sherpa_onnx_transducer' | 'llama_cpp';
  files: {
    role: 'encoder' | 'decoder' | 'joiner' | 'tokens' | 'weights';
    path: string;
    sha256: string;
    sizeBytes: number;
  }[];
  license: string;
  modelType?: 'nemo_transducer' | null;
  languages?: string[];
}

/** Persisted settings shared with the Rust AppConfig contract. */
export interface AppConfig {
  schemaVersion: number;
  models: {
    asr: AsrModel;
    rewriter: RewriteModel;
    customAsr: HuggingFaceProfile | null;
    customRewriter: HuggingFaceProfile | null;
  };
  cleanupEnabled: boolean;
  audioFrontend: AudioFrontendConfig;
  language: Language;
  modelDirectory: string | null;
  cleanupPreset: CleanupPreset;
  customCleanupInstruction: string;
  rewriteQuantization: RewriteQuantization;
  computeBackend: ComputeBackend;
  maxRecordingSeconds: number;
  globalShortcut: string;
  recordingMode: 'toggle' | 'hold';
  holdShortcut: string | null;
  toggleShortcut: string | null;
  dictationOutput: 'app' | 'paste';
  floatingBarEnabled: boolean;
  captureDesign: CaptureDesign;
  notificationsEnabled: boolean;
  collectTrainingData: boolean;
  textEditObservation: boolean;
  improvementMode: ImprovementMode;
}

const LANGUAGE_OPTIONS: readonly LanguageOption[] = [
  {
    value: 'auto',
    label: 'Detect automatically',
    description: 'Best for mixed-language speech when the model supports it',
  },
  { value: 'en', label: 'English', description: 'Transcribe supported English speech' },
  { value: 'fr', label: 'French', description: 'Transcribe supported French speech' },
  { value: 'hi', label: 'Hindi', description: 'Transcribe supported Hindi speech' },
];

/** The app currently exposes four language modes; model metadata narrows them. */
export function transcriptionLanguageOptions(models: AppConfig['models']): LanguageOption[] {
  if (models.customAsr) {
    const declared = new Set(models.customAsr.languages ?? []);
    return LANGUAGE_OPTIONS.filter(
      (option) => option.value === 'auto' || declared.has(option.value),
    );
  }
  const supported: readonly Language[] =
    models.asr === 'LiquidAI/LFM2.5-Audio-1.5B'
      ? ['en']
      : models.asr === 'CohereLabs/cohere-transcribe-03-2026'
        ? ['en', 'fr']
        : models.asr === 'nvidia/parakeet-tdt-0.6b-v3'
          ? ['auto', 'en', 'fr']
          : ['auto', 'en', 'fr', 'hi'];
  return LANGUAGE_OPTIONS.filter((option) => supported.includes(option.value));
}

export function defaultSettings(): AppConfig {
  return {
    schemaVersion: 1,
    models: {
      asr: 'CohereLabs/cohere-transcribe-03-2026',
      rewriter: 'Qwen/Qwen3.5-0.8B',
      customAsr: null,
      customRewriter: null,
    },
    cleanupEnabled: true,
    audioFrontend: automaticAudioFrontend(),
    language: 'en',
    modelDirectory: null,
    cleanupPreset: 'minimal',
    customCleanupInstruction: '',
    rewriteQuantization: 'q4_0',
    computeBackend: 'auto',
    maxRecordingSeconds: 0,
    globalShortcut: 'CommandOrControl+Shift+Space',
    recordingMode: 'toggle',
    holdShortcut: null,
    toggleShortcut: null,
    dictationOutput: 'app',
    floatingBarEnabled: true,
    captureDesign: 'split',
    notificationsEnabled: false,
    collectTrainingData: true,
    textEditObservation: false,
    improvementMode: 'review_required',
  };
}

export function requiresCompletionReport(models: AppConfig['models']): boolean {
  return (
    models.customAsr === null &&
    (models.asr === 'Qwen/Qwen3-ASR-0.6B' || models.asr === 'CohereLabs/cohere-transcribe-03-2026')
  );
}

export function recordingShortcuts(
  settings: Pick<AppConfig, 'holdShortcut' | 'toggleShortcut' | 'globalShortcut' | 'recordingMode'>,
): { hold: string; toggle: string } {
  let hold =
    settings.holdShortcut ?? (settings.recordingMode === 'hold' ? settings.globalShortcut : null);
  let toggle =
    settings.toggleShortcut ??
    (settings.recordingMode === 'toggle' ? settings.globalShortcut : null);
  hold ??= ['Alt+Shift+D', 'CommandOrControl+Shift+D', 'CommandOrControl+Shift+Space'].find(
    (key) => !sameShortcut(key, toggle),
  )!;
  toggle ??= ['CommandOrControl+Shift+Space', 'CommandOrControl+Shift+D', 'Alt+Shift+D'].find(
    (key) => !sameShortcut(key, hold),
  )!;
  return { hold, toggle };
}

export function effectiveRecordingSeconds(_settings: AppConfig): number {
  // Retain the old stored field for compatibility, but never apply it as a quota.
  return 0;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isOneOf(value: unknown, options: readonly string[]): boolean {
  return typeof value === 'string' && options.includes(value);
}

const safeComponent = /^[a-zA-Z0-9_.-]+$/;
function validComponent(value: string): boolean {
  return safeComponent.test(value) && value !== '.' && value !== '..';
}

/** Validate the native model profile contract before modifying the settings draft. */
export function validateModelProfile(value: unknown, expectedRole?: ModelProfileRole): string[] {
  if (!isRecord(value)) return ['A model profile must be a JSON object.'];
  const errors: string[] = [];
  const profileFields = [
    'id',
    'name',
    'repository',
    'revision',
    'runtime',
    'files',
    'license',
    'modelType',
    'languages',
  ];
  if (Object.keys(value).some((key) => !profileFields.includes(key)))
    errors.push(
      'The profile contains unrecognized fields. Only model profile metadata is accepted.',
    );
  if (typeof value.id !== 'string' || !validComponent(value.id) || value.id.length > 80)
    errors.push('Profile ID must use 1–80 letters, digits, dots, underscores or hyphens.');
  if (
    typeof value.name !== 'string' ||
    !value.name.trim() ||
    [...value.name].length > 120 ||
    /\p{Cc}/u.test(value.name)
  )
    errors.push('Give the profile a printable name of 1–120 characters.');
  if (
    typeof value.repository !== 'string' ||
    value.repository.length > 200 ||
    value.repository.split('/').length !== 2 ||
    !value.repository.split('/').every(validComponent)
  )
    errors.push('Repository must be a Hugging Face owner/name of at most 200 characters.');
  if (typeof value.revision !== 'string' || !/^[a-f0-9]{40}$/.test(value.revision))
    errors.push(
      'Pin the revision to a 40-character lowercase commit hash; branches such as main are not accepted.',
    );
  if (
    typeof value.license !== 'string' ||
    !value.license.trim() ||
    new TextEncoder().encode(value.license).byteLength > 500 ||
    /\p{Cc}/u.test(value.license)
  )
    errors.push(
      'Include a printable model license identifier or description of at most 500 bytes.',
    );
  const asr = value.runtime === 'sherpa_onnx_transducer';
  const rewriter = value.runtime === 'llama_cpp';
  if (!asr && !rewriter)
    errors.push(
      'Supported runtimes are sherpa_onnx_transducer for speech and llama_cpp for writing models.',
    );
  if ((expectedRole === 'asr' && !asr) || (expectedRole === 'rewriter' && !rewriter))
    errors.push(
      `This profile cannot be used as the ${expectedRole === 'asr' ? 'speech' : 'writing'} model.`,
    );
  if (asr && value.modelType !== 'nemo_transducer')
    errors.push('Speech profiles must declare modelType: nemo_transducer.');
  if (rewriter && value.modelType !== undefined && value.modelType !== null)
    errors.push('Writing model profiles must omit modelType or set it to null.');
  if (
    value.languages !== undefined &&
    (!Array.isArray(value.languages) ||
      value.languages.length > 64 ||
      !value.languages.every(
        (language) => typeof language === 'string' && /^[a-zA-Z0-9-]{1,24}$/.test(language),
      ))
  )
    errors.push(
      'Languages must be a list of at most 64 language codes using 1–24 letters, digits or hyphens.',
    );
  if (asr && (!Array.isArray(value.languages) || !value.languages.length))
    errors.push('Speech profiles must declare their supported languages.');

  if (!Array.isArray(value.files)) {
    errors.push('Files must be a list of pinned model files.');
  } else {
    const required = asr ? ['encoder', 'decoder', 'joiner', 'tokens'] : ['weights'];
    const files = value.files.filter(isRecord);
    if (
      files.length !== value.files.length ||
      files.length !== required.length ||
      required.some((role) => files.filter((file) => file.role === role).length !== 1)
    ) {
      errors.push(
        asr
          ? 'Speech profiles require exactly one encoder, decoder, joiner and tokens file.'
          : 'Writing model profiles require exactly one weights file.',
      );
    }
    const paths = new Set<string>();
    for (const [index, file] of files.entries()) {
      const label = `File ${index + 1}`;
      if (Object.keys(file).some((key) => !['role', 'path', 'sha256', 'sizeBytes'].includes(key)))
        errors.push(`${label}: unrecognized fields are not accepted.`);
      if (
        typeof file.path !== 'string' ||
        file.path.length > 512 ||
        !file.path.split('/').every(validComponent)
      )
        errors.push(
          `${label}: use a relative repository path of at most 512 characters without empty, . or .. components.`,
        );
      else {
        if (paths.has(file.path)) errors.push(`${label}: each file path must be unique.`);
        paths.add(file.path);
        if (rewriter && (!file.path.endsWith('.gguf') || /-\d{5}-of-\d{5}\.gguf$/.test(file.path)))
          errors.push(`${label}: writing model weights must be a single, unsplit .gguf file.`);
        if (asr && !file.path.endsWith(file.role === 'tokens' ? '.txt' : '.onnx'))
          errors.push(
            `${label}: speech weights must be .onnx files and tokens must be a .txt file.`,
          );
      }
      if (typeof file.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(file.sha256))
        errors.push(`${label}: include the 64-character lowercase SHA-256 hash.`);
      if (
        typeof file.sizeBytes !== 'number' ||
        !Number.isSafeInteger(file.sizeBytes) ||
        file.sizeBytes <= 0 ||
        file.sizeBytes > 50_000_000_000
      )
        errors.push(`${label}: sizeBytes must be a whole number from 1 to 50,000,000,000.`);
    }
  }
  return errors;
}

export function parseModelProfileJson(json: string): HuggingFaceProfile {
  if (new TextEncoder().encode(json).byteLength > 32 * 1024)
    throw new Error('Model profile JSON must be at most 32 KiB.');
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    throw new Error('This is not valid JSON. Paste a single model profile object.');
  }
  const errors = validateModelProfile(value);
  if (errors.length) throw new Error(errors.join(' '));
  return value as HuggingFaceProfile;
}

/** Validate stored and incoming data as well as the editable form. */
export function validateSettings(value: unknown): string[] {
  if (!isRecord(value)) return ['Settings must be an object.'];
  const errors: string[] = [];
  if (value.schemaVersion !== 1) errors.push('This settings version is not supported.');

  if (!isRecord(value.models)) {
    errors.push('Select valid speech and writing models.');
  } else {
    if (!isOneOf(value.models.asr, Object.keys(ASR_MODELS)))
      errors.push('Select a supported speech recognition model.');
    if (!isOneOf(value.models.rewriter, Object.keys(CLEANUP_MODELS)))
      errors.push('Select a supported writing model.');
    if (value.models.customAsr !== null)
      errors.push(...validateModelProfile(value.models.customAsr, 'asr'));
    if (value.models.customRewriter !== null)
      errors.push(...validateModelProfile(value.models.customRewriter, 'rewriter'));
    if (
      value.models.customAsr === null &&
      value.models.asr === 'nvidia/parakeet-tdt-0.6b-v3' &&
      value.language === 'hi'
    ) {
      errors.push('Parakeet does not support Hindi. Select Nemotron or change the language.');
    }
    if (
      value.models.customAsr === null &&
      value.models.asr === 'CohereLabs/cohere-transcribe-03-2026' &&
      value.language !== 'en' &&
      value.language !== 'fr'
    ) {
      errors.push(
        'Cohere requires an explicit language. Select English or French; automatic detection and Hindi are unavailable.',
      );
    }
    if (
      value.models.customAsr === null &&
      value.models.asr === 'LiquidAI/LFM2.5-Audio-1.5B' &&
      value.language !== 'en'
    ) {
      errors.push(
        'LFM2.5 Audio requires English; automatic detection, French and Hindi are unavailable.',
      );
    }
    if (
      value.models.customAsr === null &&
      (value.models.asr === 'Qwen/Qwen3-ASR-0.6B' ||
        value.models.asr === 'CohereLabs/cohere-transcribe-03-2026') &&
      value.computeBackend !== 'auto' &&
      value.computeBackend !== 'cpu' &&
      !(
        value.models.asr === 'Qwen/Qwen3-ASR-0.6B' &&
        value.computeBackend === 'metal' &&
        value.language === 'en'
      )
    ) {
      errors.push('The selected speech model supports Automatic and CPU only.');
    }
    if (
      isRecord(value.models.customAsr) &&
      value.language !== 'auto' &&
      Array.isArray(value.models.customAsr.languages) &&
      !value.models.customAsr.languages.includes(value.language)
    )
      errors.push('The custom speech profile does not declare support for the selected language.');
  }

  if (
    !isRecord(value.audioFrontend) ||
    typeof value.audioFrontend.vadEnabled !== 'boolean' ||
    typeof value.audioFrontend.autoLevelEnabled !== 'boolean' ||
    !isOneOf(value.audioFrontend.noiseReduction, ['off', 'dpdfnet4', 'dpdfnet8']) ||
    Object.keys(value.audioFrontend).some(
      (key) => !['vadEnabled', 'autoLevelEnabled', 'noiseReduction'].includes(key),
    )
  )
    errors.push('Choose valid audio processing settings.');
  if (typeof value.cleanupEnabled !== 'boolean')
    errors.push('Choose whether the writing model is enabled.');
  if (!isOneOf(value.language, ['auto', 'en', 'fr', 'hi']))
    errors.push('Select a supported language.');
  if (!isOneOf(value.cleanupPreset, ['minimal', 'balanced']))
    errors.push('Select a writing style.');
  if (!isOneOf(value.rewriteQuantization, Object.keys(CLEANUP_QUANTIZATIONS)))
    errors.push('Select a supported writing model quantization.');
  if (!isOneOf(value.computeBackend, ['auto', 'cpu', 'metal', 'cuda', 'vulkan', 'rocm']))
    errors.push('Select a supported inference backend.');

  if (
    value.modelDirectory !== null &&
    (typeof value.modelDirectory !== 'string' ||
      !value.modelDirectory.trim() ||
      [...value.modelDirectory].length > 4096)
  ) {
    errors.push('Use the default model folder or enter a path of at most 4,096 characters.');
  }
  if (
    typeof value.customCleanupInstruction !== 'string' ||
    [...value.customCleanupInstruction].length > 4000
  ) {
    errors.push('Keep additional writing instructions within 4,000 characters.');
  }
  if (
    typeof value.maxRecordingSeconds !== 'number' ||
    !Number.isInteger(value.maxRecordingSeconds) ||
    value.maxRecordingSeconds < 0 ||
    value.maxRecordingSeconds > 0xffffffff
  ) {
    errors.push('The legacy recording setting must be an unsigned whole number.');
  }
  if (!shortcutIdentity(value.globalShortcut))
    errors.push('Choose a shortcut with Ctrl, Command/Super or Alt/Option and a supported key.');
  if (typeof value.floatingBarEnabled !== 'boolean')
    errors.push('Choose whether the quick-capture bar is shown.');
  if (!isOneOf(value.captureDesign, CAPTURE_DESIGNS))
    errors.push('Select a supported capture design.');
  if (!isOneOf(value.recordingMode, ['toggle', 'hold']))
    errors.push('Choose Hold or Toggle for the recording shortcut.');
  for (const field of ['holdShortcut', 'toggleShortcut']) {
    if (value[field] !== null && !shortcutIdentity(value[field]))
      errors.push(
        'Choose Hold and Toggle shortcuts with Ctrl, Command/Super or Alt/Option and a supported key.',
      );
  }
  const shortcuts = recordingShortcuts(value as unknown as AppConfig);
  if (sameShortcut(shortcuts.hold, shortcuts.toggle))
    errors.push('Hold and Toggle must use different shortcuts.');
  if (!isOneOf(value.dictationOutput, ['app', 'paste']))
    errors.push('Choose where completed dictation goes.');
  if (typeof value.notificationsEnabled !== 'boolean')
    errors.push('Choose whether desktop notifications are enabled.');
  if (typeof value.textEditObservation !== 'boolean')
    errors.push('Choose whether to observe TextEdit corrections.');
  if (typeof value.collectTrainingData !== 'boolean')
    errors.push('Choose whether training data is collected.');
  if (!isOneOf(value.improvementMode, ['review_required', 'automatic']))
    errors.push('Select an improvement policy.');
  return errors;
}

export function parseSettings(value: unknown): AppConfig {
  const errors = validateSettings(value);
  if (errors.length) throw new Error(errors.join(' '));
  return { ...(value as AppConfig), audioFrontend: automaticAudioFrontend() };
}

/** Match Rust serde defaults when loading older settings with absent fields. */
export function parseStoredSettings(value: unknown): AppConfig {
  if (!isRecord(value)) return parseSettings(value);

  // Speaker analysis is automatic; discard the retired preference, including false.
  const stored = { ...value };
  delete stored.speakerDiarization;
  const defaults = defaultSettings();
  const models = isRecord(value.models)
    ? { ...defaults.models, ...value.models }
    : Object.hasOwn(value, 'models')
      ? value.models
      : defaults.models;

  // Explicit invalid values, including null, override defaults and still fail validation.
  const audioFrontend = isRecord(value.audioFrontend)
    ? { ...defaults.audioFrontend, ...value.audioFrontend }
    : Object.hasOwn(value, 'audioFrontend')
      ? value.audioFrontend
      : defaults.audioFrontend;
  return parseSettings({ ...defaults, ...stored, models, audioFrontend });
}
