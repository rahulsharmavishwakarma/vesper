import { APPLE_GPU_ONLY_SPEECH_MODELS } from './accelerators.ts';
import { sameShortcut, shortcutIdentity } from './recording-shortcuts.ts';
import languageManifest from '../../../../models/languages.json' with { type: 'json' };
import modelCatalog from '../../../../models/catalog.json' with { type: 'json' };

export interface CatalogModel {
  readonly name: string;
  readonly role: string;
  readonly status: string;
  readonly creator: string;
  readonly picker_blurb: string;
}

function parseModelCatalog(): Record<string, CatalogModel> {
  const root = modelCatalog as unknown as { models?: unknown };
  const table: Record<string, CatalogModel> = {};
  if (!Array.isArray(root.models)) return table;
  for (const entry of root.models) {
    if (!entry || typeof entry !== 'object') continue;
    const record = entry as Record<string, unknown>;
    if (typeof record.id !== 'string' || typeof record.name !== 'string') continue;
    table[record.id] = {
      name: record.name,
      role: typeof record.role === 'string' ? record.role : '',
      status: typeof record.status === 'string' ? record.status : '',
      creator: typeof record.creator === 'string' ? record.creator : '',
      picker_blurb: typeof record.picker_blurb === 'string' ? record.picker_blurb : '',
    };
  }
  return table;
}

/** Model display source of truth (models/catalog.json). The selectable-id
 * contract below mirrors the backend enum; the backend rejects unknown ids. */
const MODEL_CATALOG: Record<string, CatalogModel> = parseModelCatalog();

/** Catalog statuses offered in product pickers; anything else stays unoffered. */
export const OFFERED_MODEL_STATUSES: ReadonlySet<string> = new Set([
  'default',
  'optional',
  'optional_fallback',
]);

export function catalogModel(id: string): CatalogModel | null {
  return MODEL_CATALOG[id] ?? null;
}

/** Offered catalog ids for a role, in catalog order. */
export function offeredModelIds(role: string): string[] {
  return Object.keys(MODEL_CATALOG).filter(
    (id) => MODEL_CATALOG[id].role === role && OFFERED_MODEL_STATUSES.has(MODEL_CATALOG[id].status),
  );
}

export const ASR_MODELS: Record<string, string> = Object.fromEntries(
  offeredModelIds('speech_recognition').map((id) => [id, MODEL_CATALOG[id].name]),
);

export type AsrModel = string;
export type Language =
  | 'auto'
  | 'ar'
  | 'bg'
  | 'yue'
  | 'zh'
  | 'hr'
  | 'cs'
  | 'da'
  | 'nl'
  | 'en'
  | 'et'
  | 'fil'
  | 'fi'
  | 'fr'
  | 'de'
  | 'el'
  | 'hi'
  | 'hu'
  | 'id'
  | 'it'
  | 'ja'
  | 'ko'
  | 'lv'
  | 'lt'
  | 'mk'
  | 'ms'
  | 'mt'
  | 'nb'
  | 'fa'
  | 'pl'
  | 'pt'
  | 'ro'
  | 'ru'
  | 'sk'
  | 'sl'
  | 'es'
  | 'sv'
  | 'th'
  | 'tr'
  | 'uk'
  | 'vi';
export type LanguageOption = { value: Language; label: string; description: string };
export type CleanupPreset = 'minimal' | 'balanced';
export type ComputeBackend = 'auto' | 'cpu' | 'metal' | 'cuda' | 'vulkan' | 'rocm';
/** Engine for a speech model with more than one; see `AsrEngine` in vesper-core. */
export type AsrEngine = 'standard' | 'unified_gguf';
export { UNIFIED_GGUF_MODELS, usesUnifiedGguf } from './accelerators.ts';
import { usesUnifiedGguf } from './accelerators.ts';

export const ASR_ENGINE_OPTIONS: { value: AsrEngine; label: string; description: string }[] = [
  {
    value: 'standard',
    label: 'Current',
    description: 'CPU file, or the Apple GPU file for English',
  },
  {
    value: 'unified_gguf',
    label: 'Unified GGUF',
    description: 'One file for CPU and every supported GPU',
  },
];
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
export const CAPTURE_DESIGNS = ['capsule', 'split', 'island'] as const;
export type CaptureDesign = (typeof CAPTURE_DESIGNS)[number];
export type ModelProfileRole = 'asr' | 'rewriter';
export const MODEL_ROLE_LABELS = { asr: 'Speech model', rewriter: 'Writing model' } as const;

/** Built-in cleanup models with pinned artifacts understood by vesper-models.
 * Display names and descriptions come from models/catalog.json; only the
 * functional runtime and quantization pins stay here. */
export const CLEANUP_MODELS = {
  'Qwen/Qwen3.5-0.8B': {
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
  (value) => {
    const display = catalogModel(value);
    return {
      value,
      label: display?.name ?? value,
      description: display?.picker_blurb ?? '',
    };
  },
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
    /** The Rust contract fills `standard` for settings saved before this choice existed. */
    asrEngine: AsrEngine;
    rewriter: RewriteModel;
    customAsr: HuggingFaceProfile | null;
    customRewriter: HuggingFaceProfile | null;
  };
  cleanupEnabled: boolean;
  audioFrontend: AudioFrontendConfig;
  language: Language;
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
  improvementMode: ImprovementMode;
}

export const LANGUAGE_OPTIONS: readonly LanguageOption[] = [
  {
    value: 'auto',
    label: 'Detect automatically',
    description: 'Let the speech model identify the spoken language',
  },
  { value: 'ar', label: 'Arabic', description: 'Transcribe supported Arabic speech' },
  { value: 'bg', label: 'Bulgarian', description: 'Transcribe supported Bulgarian speech' },
  { value: 'yue', label: 'Cantonese', description: 'Transcribe supported Cantonese speech' },
  { value: 'zh', label: 'Chinese (Mandarin)', description: 'Transcribe supported Mandarin speech' },
  { value: 'hr', label: 'Croatian', description: 'Transcribe supported Croatian speech' },
  { value: 'cs', label: 'Czech', description: 'Transcribe supported Czech speech' },
  { value: 'da', label: 'Danish', description: 'Transcribe supported Danish speech' },
  { value: 'nl', label: 'Dutch', description: 'Transcribe supported Dutch speech' },
  { value: 'en', label: 'English', description: 'Transcribe supported English speech' },
  { value: 'et', label: 'Estonian', description: 'Transcribe supported Estonian speech' },
  { value: 'fil', label: 'Filipino', description: 'Transcribe supported Filipino speech' },
  { value: 'fi', label: 'Finnish', description: 'Transcribe supported Finnish speech' },
  { value: 'fr', label: 'French', description: 'Transcribe supported French speech' },
  { value: 'de', label: 'German', description: 'Transcribe supported German speech' },
  { value: 'el', label: 'Greek', description: 'Transcribe supported Greek speech' },
  { value: 'hi', label: 'Hindi', description: 'Transcribe supported Hindi speech' },
  { value: 'hu', label: 'Hungarian', description: 'Transcribe supported Hungarian speech' },
  { value: 'id', label: 'Indonesian', description: 'Transcribe supported Indonesian speech' },
  { value: 'it', label: 'Italian', description: 'Transcribe supported Italian speech' },
  { value: 'ja', label: 'Japanese', description: 'Transcribe supported Japanese speech' },
  { value: 'ko', label: 'Korean', description: 'Transcribe supported Korean speech' },
  { value: 'lv', label: 'Latvian', description: 'Transcribe supported Latvian speech' },
  { value: 'lt', label: 'Lithuanian', description: 'Transcribe supported Lithuanian speech' },
  { value: 'mk', label: 'Macedonian', description: 'Transcribe supported Macedonian speech' },
  { value: 'ms', label: 'Malay', description: 'Transcribe supported Malay speech' },
  { value: 'mt', label: 'Maltese', description: 'Transcribe supported Maltese speech' },
  {
    value: 'nb',
    label: 'Norwegian (Bokmål)',
    description: 'Transcribe supported Norwegian speech',
  },
  { value: 'fa', label: 'Persian', description: 'Transcribe supported Persian speech' },
  { value: 'pl', label: 'Polish', description: 'Transcribe supported Polish speech' },
  { value: 'pt', label: 'Portuguese', description: 'Transcribe supported Portuguese speech' },
  { value: 'ro', label: 'Romanian', description: 'Transcribe supported Romanian speech' },
  { value: 'ru', label: 'Russian', description: 'Transcribe supported Russian speech' },
  { value: 'sk', label: 'Slovak', description: 'Transcribe supported Slovak speech' },
  { value: 'sl', label: 'Slovenian', description: 'Transcribe supported Slovenian speech' },
  { value: 'es', label: 'Spanish', description: 'Transcribe supported Spanish speech' },
  { value: 'sv', label: 'Swedish', description: 'Transcribe supported Swedish speech' },
  { value: 'th', label: 'Thai', description: 'Transcribe supported Thai speech' },
  { value: 'tr', label: 'Turkish', description: 'Transcribe supported Turkish speech' },
  { value: 'uk', label: 'Ukrainian', description: 'Transcribe supported Ukrainian speech' },
  { value: 'vi', label: 'Vietnamese', description: 'Transcribe supported Vietnamese speech' },
];

function declaredLanguages(profile: HuggingFaceProfile): Set<string> {
  return new Set((profile.languages ?? []).map((tag) => tag.split('-')[0].toLowerCase()));
}

interface ModelLanguages {
  languages: readonly Language[];
  autoDetect: boolean;
}

/** Vendor-declared transcription languages per built-in model (models/languages.json).
 * The backend agreement test pins these lists to the adapter capability tables. */
const MODEL_LANGUAGES: Record<string, ModelLanguages> = (() => {
  const known = new Set(LANGUAGE_OPTIONS.map((option) => option.value));
  const table: Record<string, ModelLanguages> = {};
  for (const [id, entry] of Object.entries(
    languageManifest as unknown as Record<string, { languages: unknown; autoDetect: unknown }>,
  )) {
    if (id.startsWith('_') || id === 'writers' || !entry || typeof entry !== 'object') continue;
    const languages = Array.isArray(entry.languages)
      ? entry.languages.filter((code): code is Language => known.has(code as Language))
      : [];
    table[id] = { languages, autoDetect: entry.autoDetect === true };
  }
  return table;
})();

interface WriterLanguages {
  languages: readonly Language[];
  unrestricted: boolean;
}

/** Built-in writer language support (models/languages.json writers section).
 * Unrestricted writers publish no per-language list; every mode stays allowed. */
const MODEL_WRITER_LANGUAGES: Record<string, WriterLanguages> = (() => {
  const known = new Set(LANGUAGE_OPTIONS.map((option) => option.value));
  const table: Record<string, WriterLanguages> = {};
  const section = (languageManifest as unknown as Record<string, unknown>).writers;
  if (!section || typeof section !== 'object') return table;
  for (const [id, entry] of Object.entries(
    section as Record<string, { languages: unknown; unrestricted: unknown }>,
  )) {
    if (!entry || typeof entry !== 'object') continue;
    const languages = Array.isArray(entry.languages)
      ? entry.languages.filter((code): code is Language => known.has(code as Language))
      : [];
    table[id] = { languages, unrestricted: entry.unrestricted === true };
  }
  return table;
})();

/** Fallback when the manifest lacks a model: the legacy four modes. */
const LEGACY_LANGUAGES: readonly Language[] = ['auto', 'en', 'fr', 'hi'];

/** The app offers each speech model's vendor-declared languages; model/backend metadata narrows them. */
export function transcriptionLanguageOptions(
  models: AppConfig['models'],
  backend: ComputeBackend = 'auto',
): LanguageOption[] {
  if (models.customAsr) {
    const declared = declaredLanguages(models.customAsr);
    return LANGUAGE_OPTIONS.filter(
      (option) => option.value === 'auto' || declared.has(option.value),
    );
  }
  if (
    backend === 'metal' &&
    !usesUnifiedGguf(models) &&
    ['nvidia/parakeet-tdt-0.6b-v3', 'Qwen/Qwen3-ASR-0.6B'].includes(models.asr)
  )
    return LANGUAGE_OPTIONS.filter((option) => option.value === 'en');
  const entry = MODEL_LANGUAGES[models.asr];
  const supported: readonly Language[] = entry
    ? [...(entry.autoDetect ? (['auto'] as const) : []), ...entry.languages]
    : LEGACY_LANGUAGES;
  return LANGUAGE_OPTIONS.filter((option) => supported.includes(option.value));
}

export function transcriptionLanguageError(config: AppConfig): string | null {
  const options = transcriptionLanguageOptions(config.models, config.computeBackend);
  if (options.some((option) => option.value === config.language)) return null;
  return `${config.models.customAsr?.name ?? catalogModel(config.models.asr)?.name ?? 'This speech model'} and the selected processing device support these languages: ${options.map((option) => option.label).join(', ')}.`;
}

export function writingLanguageError(config: AppConfig): string | null {
  const profile = config.models.customRewriter;
  if (!config.cleanupEnabled) return null;
  if (!profile) {
    // Built-in writer: unrestricted writers publish no per-language list,
    // so every mode stays allowed and nothing is invented.
    const writer = MODEL_WRITER_LANGUAGES[config.models.rewriter];
    if (!writer || writer.unrestricted) return null;
    if (config.language === 'auto')
      return 'Choose an explicit language supported by the writing model, or turn polishing off. Automatic detection does not establish writer compatibility.';
    return writer.languages.includes(config.language)
      ? null
      : 'The selected writing model does not declare support for this language. Choose a compatible writer or turn polishing off.';
  }
  const declared = declaredLanguages(profile);
  if (!declared.size)
    return 'The writing profile has no declared languages. Reselect a writing preset with language support, or turn polishing off.';
  if (config.language === 'auto')
    return 'Choose an explicit language supported by the writing model, or turn polishing off. Automatic detection does not establish writer compatibility.';
  return declared.has(config.language)
    ? null
    : 'The selected writing model does not declare support for this language. Choose a compatible writer or turn polishing off.';
}

/** Shared memory is model independent; spoken/output languages still need compatible weights. */
/** Nearest supported transcription mode after a model switch: keep the current
 * language when offered, else automatic detection, else English, else the
 * first offered explicit language. Never leaves a stranded selection behind. */
export function fallbackLanguage(config: AppConfig): Language {
  const offered = pipelineLanguageOptions(config).map((option) => option.value);
  if (offered.includes(config.language)) return config.language;
  if (offered.includes('auto')) return 'auto';
  if (offered.includes('en')) return 'en';
  return offered.find((code) => code !== 'auto') ?? config.language;
}

export function pipelineLanguageOptions(config: AppConfig): LanguageOption[] {
  const speech = transcriptionLanguageOptions(config.models, config.computeBackend);
  if (!config.cleanupEnabled) return speech;
  if (config.models.customRewriter) {
    const declared = declaredLanguages(config.models.customRewriter);
    return speech.filter((option) => option.value !== 'auto' && declared.has(option.value));
  }
  const writer = MODEL_WRITER_LANGUAGES[config.models.rewriter];
  if (!writer || writer.unrestricted) return speech;
  const allowed = new Set(writer.languages);
  return speech.filter((option) => option.value !== 'auto' && allowed.has(option.value));
}

export interface ModelLanguageSummary {
  languages: readonly Language[];
  unrestricted: boolean;
}

/** Explicit vendor-declared languages for a built-in speech model (no `auto` mode). */
export function speechModelLanguages(id: string): readonly Language[] {
  return MODEL_LANGUAGES[id]?.languages ?? [];
}

/** Language support for a built-in writer; unrestricted means no per-language list. */
export function writerLanguageSummary(id: string): ModelLanguageSummary {
  const entry = MODEL_WRITER_LANGUAGES[id];
  return entry
    ? { languages: entry.languages, unrestricted: entry.unrestricted }
    : { languages: [], unrestricted: false };
}

export function defaultSettings(): AppConfig {
  return {
    schemaVersion: 1,
    models: {
      asr: 'CohereLabs/cohere-transcribe-03-2026',
      asrEngine: 'standard',
      rewriter: 'Qwen/Qwen3.5-0.8B',
      customAsr: null,
      customRewriter: null,
    },
    cleanupEnabled: true,
    audioFrontend: automaticAudioFrontend(),
    language: 'en',
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
export function validateSettings(value: unknown, checkWriterLanguage = true): string[] {
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
    if (errors.length === 0) {
      const config = value as unknown as AppConfig;
      const speechError = transcriptionLanguageError(config);
      const writerError = checkWriterLanguage ? writingLanguageError(config) : null;
      if (speechError) errors.push(speechError);
      if (writerError) errors.push(writerError);
    }
    if (
      value.models.customAsr === null &&
      (value.models.asr === 'Qwen/Qwen3-ASR-0.6B' ||
        value.models.asr === 'CohereLabs/cohere-transcribe-03-2026') &&
      value.computeBackend !== 'auto' &&
      value.computeBackend !== 'cpu' &&
      !(value.models.asr === 'Qwen/Qwen3-ASR-0.6B' && value.computeBackend === 'metal')
    ) {
      errors.push('The selected speech model supports Automatic and CPU only.');
    }
    if (
      value.models.customAsr === null &&
      APPLE_GPU_ONLY_SPEECH_MODELS.includes(String(value.models.asr)) &&
      value.computeBackend !== 'auto' &&
      value.computeBackend !== 'metal'
    ) {
      errors.push('The selected speech model runs only on the Apple GPU (Automatic or Metal).');
    }
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
  if (
    !isOneOf(
      value.language,
      LANGUAGE_OPTIONS.map((option) => option.value),
    )
  )
    errors.push('Select a supported language.');
  if (!isOneOf(value.cleanupPreset, ['minimal', 'balanced']))
    errors.push('Select a writing style.');
  if (!isOneOf(value.rewriteQuantization, Object.keys(CLEANUP_QUANTIZATIONS)))
    errors.push('Select a supported writing model quantization.');
  if (!isOneOf(value.computeBackend, ['auto', 'cpu', 'metal', 'cuda', 'vulkan', 'rocm']))
    errors.push('Select a supported inference backend.');

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
  if (typeof value.collectTrainingData !== 'boolean')
    errors.push('Choose whether training data is collected.');
  if (!isOneOf(value.improvementMode, ['review_required', 'automatic']))
    errors.push('Select an improvement policy.');
  return errors;
}

export function parseSettings(value: unknown): AppConfig {
  // Legacy writer profiles must remain readable so the user can repair them.
  // Saves and native rewriting enforce compatibility independently.
  const errors = validateSettings(value, false);
  if (errors.length) throw new Error(errors.join(' '));
  return { ...(value as AppConfig), audioFrontend: automaticAudioFrontend() };
}

/** Match Rust serde defaults when loading older settings with absent fields. */
export function parseStoredSettings(value: unknown): AppConfig {
  if (!isRecord(value)) return parseSettings(value);

  // Speaker analysis is automatic; discard the retired preference, including false.
  // TextEdit correction observation was removed; discard the retired preference.
  const stored = { ...value };
  delete stored.speakerDiarization;
  delete stored.textEditObservation;
  // The retired overlay layouts fall back to the Split default instead of
  // failing validation; anything else invalid still fails below.
  const retiredDesigns = ['ribbon', 'underline', 'studio', 'margin', 'counter'];
  if (typeof stored.captureDesign === 'string' && retiredDesigns.includes(stored.captureDesign))
    delete stored.captureDesign;
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
