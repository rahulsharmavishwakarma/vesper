import entries from '../../../../models/presets.json' with { type: 'json' };
import {
  ASR_MODELS,
  CLEANUP_MODEL_OPTIONS,
  CLEANUP_MODELS,
  LANGUAGE_OPTIONS,
  catalogModel,
  offeredModelIds,
  parseModelProfileJson,
  speechModelLanguages,
  writerLanguageSummary,
  type AppConfig,
  type HuggingFaceProfile,
  type ModelProfileRole,
} from './config.ts';

export interface ModelPreset {
  description: string;
  precision: string;
  sourceUrl: string;
  profile: HuggingFaceProfile;
}

/** Catalog shortcuts to the existing pinned-profile path; no new runtime or settings schema. */
export const MODEL_PRESETS: ModelPreset[] = (
  entries as (Omit<ModelPreset, 'profile'> & { profile: unknown })[]
).map((entry) => ({ ...entry, profile: parseModelProfileJson(JSON.stringify(entry.profile)) }));

// Original creators for reviewed catalog entries, not the download host,
// runtime, accelerator, or a namespace inferred from an imported profile.
const MODEL_CREATORS: Record<string, string> = {
  'nvidia/parakeet-tdt-0.6b-v3': 'NVIDIA',
  'nvidia/nemotron-3.5-asr-streaming-0.6b': 'NVIDIA',
  'Qwen/Qwen3-ASR-0.6B': 'Qwen · Alibaba',
  'CohereLabs/cohere-transcribe-03-2026': 'Cohere',
  'LiquidAI/LFM2.5-Audio-1.5B': 'Liquid AI',
  'Qwen/Qwen3.5-0.8B': 'Qwen · Alibaba',
  'nvidia/parakeet-tdt-0.6b-v2': 'NVIDIA',
  'nvidia/parakeet-unified-en-0.6b': 'NVIDIA',
  'Qwen/Qwen2.5-0.5B-Instruct': 'Qwen · Alibaba',
  'Qwen/Qwen2.5-1.5B-Instruct': 'Qwen · Alibaba',
  'HuggingFaceTB/SmolLM2-360M-Instruct': 'Hugging Face',
  'HuggingFaceTB/SmolLM2-1.7B-Instruct': 'Hugging Face',
  'google/gemma-3-1b-it': 'Google',
};
const UNKNOWN_CREATOR = 'Creator not specified';
function presetCreator(preset: ModelPreset): string {
  const source = new URL(preset.sourceUrl);
  if (source.hostname === 'huggingface.co')
    return MODEL_CREATORS[source.pathname.slice(1)] ?? UNKNOWN_CREATOR;
  return preset.sourceUrl ===
    'https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/stt_multilingual_fastconformer_hybrid_large_pc_blend_eu'
    ? 'NVIDIA'
    : UNKNOWN_CREATOR;
}

function runtimeFor(role: ModelProfileRole) {
  return role === 'asr' ? 'sherpa_onnx_transducer' : 'llama_cpp';
}

function selectedProfile(models: AppConfig['models'], role: ModelProfileRole) {
  return role === 'asr' ? models.customAsr : models.customRewriter;
}

// Rust may reorder JSON keys or fill optional fields when persisting a profile.
// Compare the full declaration, not just an ID that an imported profile can reuse.
function signature(profile: HuggingFaceProfile): string {
  return JSON.stringify([
    profile.id,
    profile.name,
    profile.repository,
    profile.revision,
    profile.runtime,
    profile.license,
    profile.modelType ?? null,
    [...(profile.languages ?? [])].sort(),
    profile.files
      .map((file) => [file.role, file.path, file.sha256, file.sizeBytes])
      .sort((left, right) => String(left[0]).localeCompare(String(right[0]))),
  ]);
}

export function sameModelProfile(left: HuggingFaceProfile, right: HuggingFaceProfile): boolean {
  return signature(left) === signature(right);
}

export function presetForProfile(profile: HuggingFaceProfile | null): ModelPreset | undefined {
  return profile
    ? MODEL_PRESETS.find((preset) => sameModelProfile(preset.profile, profile))
    : undefined;
}

export function selectedModelValue(models: AppConfig['models'], role: ModelProfileRole): string {
  const profile = selectedProfile(models, role);
  if (!profile) return role === 'asr' ? models.asr : models.rewriter;
  return presetForProfile(profile) ? `preset:${profile.id}` : 'custom';
}

export interface ModelOptionLanguage {
  value: string;
  label: string;
}

export interface ModelOption {
  value: string;
  label: string;
  creator: string;
  description: string;
  /** Explicit vendor-declared languages for the picker chip; empty when unknown. */
  languages: ModelOptionLanguage[];
  /** True when the writer publishes no per-language list (chip reads "Many languages"). */
  unrestrictedLanguages: boolean;
}

const LANGUAGE_LABELS: Map<string, string> = new Map(
  LANGUAGE_OPTIONS.map((option) => [option.value, option.label]),
);

/** Picker chips show explicit languages only; `auto` is a mode, not a language. */
function languageChips(codes: readonly string[]): ModelOptionLanguage[] {
  return codes
    .filter((code) => code !== 'auto' && LANGUAGE_LABELS.has(code))
    .map((code) => ({ value: code, label: LANGUAGE_LABELS.get(code) ?? code }));
}

export function modelOptions(models: AppConfig['models'], role: ModelProfileRole): ModelOption[] {
  // Display comes from models/catalog.json; only the selectable-id contract
  // (ASR_MODELS keys, CLEANUP_MODELS keys) stays code-side with the backend.
  const builtIns: ModelOption[] =
    role === 'asr'
      ? offeredModelIds('speech_recognition').map((value) => {
          const display = catalogModel(value);
          return {
            value,
            label: display?.name ?? value,
            creator: display?.creator || UNKNOWN_CREATOR,
            description: display?.picker_blurb ?? '',
            languages: languageChips(speechModelLanguages(value)),
            unrestrictedLanguages: false,
          };
        })
      : CLEANUP_MODEL_OPTIONS.map((option) => {
          const display = catalogModel(option.value);
          const summary = writerLanguageSummary(option.value);
          return {
            value: option.value,
            label: display?.name ?? option.label,
            description: display?.picker_blurb ?? option.description,
            creator: display?.creator || UNKNOWN_CREATOR,
            languages: languageChips(summary.languages),
            unrestrictedLanguages: summary.unrestricted,
          };
        });
  const presets: ModelOption[] = MODEL_PRESETS.filter(
    (preset) => preset.profile.runtime === runtimeFor(role),
  ).map((preset) => ({
    value: `preset:${preset.profile.id}`,
    label: preset.profile.name,
    creator: presetCreator(preset),
    description: preset.description,
    languages: languageChips(preset.profile.languages ?? []),
    unrestrictedLanguages: false,
  }));
  const profile = selectedProfile(models, role);
  const custom: ModelOption[] =
    profile && !presetForProfile(profile)
      ? [
          {
            value: 'custom',
            label: profile.name,
            creator: UNKNOWN_CREATOR,
            description: 'Imported model profile',
            languages: languageChips(profile.languages ?? []),
            unrestrictedLanguages: false,
          },
        ]
      : [];
  return [...builtIns, ...presets, ...custom];
}

export function selectModel(
  models: AppConfig['models'],
  role: ModelProfileRole,
  value: string,
): AppConfig['models'] {
  if (value === 'custom' && selectedProfile(models, role)) return models;
  const next = { ...models };
  const preset = MODEL_PRESETS.find(
    (entry) => `preset:${entry.profile.id}` === value && entry.profile.runtime === runtimeFor(role),
  );
  if (preset) {
    // Settings edits must never mutate the shared preset or the other pipeline stage.
    const profile = structuredClone(preset.profile);
    if (role === 'asr') next.customAsr = profile;
    else next.customRewriter = profile;
  } else if (role === 'asr' && Object.hasOwn(ASR_MODELS, value)) {
    next.asr = value as AppConfig['models']['asr'];
    next.customAsr = null;
  } else if (role === 'rewriter' && Object.hasOwn(CLEANUP_MODELS, value)) {
    next.rewriter = value as AppConfig['models']['rewriter'];
    next.customRewriter = null;
  } else {
    throw new Error('Unknown model selection for this pipeline stage.');
  }
  return next;
}
