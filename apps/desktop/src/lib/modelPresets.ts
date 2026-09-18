import entries from '../../../../models/presets.json' with { type: 'json' };
import {
  ASR_MODELS,
  CLEANUP_MODEL_OPTIONS,
  CLEANUP_MODELS,
  parseModelProfileJson,
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

export function modelOptions(models: AppConfig['models'], role: ModelProfileRole) {
  const builtIns =
    role === 'asr'
      ? Object.entries(ASR_MODELS).map(([value, label]) => ({
          value,
          label,
          creator: MODEL_CREATORS[value] ?? UNKNOWN_CREATOR,
          description: value.includes('nemotron')
            ? 'Multilingual, including Hindi'
            : value === 'Qwen/Qwen3-ASR-0.6B'
              ? 'Multilingual, including Hindi · CPU'
              : value === 'CohereLabs/cohere-transcribe-03-2026'
                ? 'Default · select English or French · CPU'
                : value === 'LiquidAI/LFM2.5-Audio-1.5B'
                  ? 'English only · completed recordings · CPU · Q4'
                  : '25 European languages · CPU',
        }))
      : CLEANUP_MODEL_OPTIONS.map((option) => ({
          ...option,
          creator: MODEL_CREATORS[option.value] ?? UNKNOWN_CREATOR,
        }));
  const presets = MODEL_PRESETS.filter((preset) => preset.profile.runtime === runtimeFor(role)).map(
    (preset) => ({
      value: `preset:${preset.profile.id}`,
      label: preset.profile.name,
      creator: presetCreator(preset),
      description: preset.description,
    }),
  );
  const profile = selectedProfile(models, role);
  return [
    ...builtIns,
    ...presets,
    ...(profile && !presetForProfile(profile)
      ? [
          {
            value: 'custom',
            label: profile.name,
            creator: UNKNOWN_CREATOR,
            description: 'Imported model profile',
          },
        ]
      : []),
  ];
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
