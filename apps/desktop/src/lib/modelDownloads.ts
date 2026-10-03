import { ASR_MODELS, CLEANUP_MODELS, type AppConfig, type ModelProfileRole } from './config.ts';
import type { InstallProgress, ModelCatalogInstallation } from './api/runtime';
import { presetForProfile, sameModelProfile } from './modelPresets.ts';

export interface DownloadTarget {
  modelId: string;
  role: ModelCatalogInstallation['role'];
  quantization: ModelCatalogInstallation['quantization'];
  name: string;
}
export interface ModelDownloadState {
  catalog: ModelCatalogInstallation[];
  checking: boolean;
  installingKey: string | null;
  installing: boolean;
  cancelling: boolean;
  details: InstallProgress | null;
  progress: string;
  error: string | null;
  feedback: string | null;
  audioSetupPaused?: boolean;
  audioSetupFailedKey?: string | null;
}

export const AUTOMATIC_AUDIO_MODELS = [
  { modelId: 'dpdfnet8', name: 'Noise reduction' },
  { modelId: 'silero-v6.2', name: 'Speech detection' },
  { modelId: 'nvidia/Nemotron-3-Diarization', name: 'Speaker timelines' },
] as const;

/** Use the native catalog's saved selection and runtime/device admission. Never
 * download speaker weights when its separately installed runtime is unavailable. */
export function automaticAudioDownloads(catalog: ModelCatalogInstallation[]): DownloadTarget[] {
  return AUTOMATIC_AUDIO_MODELS.flatMap(({ modelId, name }) => {
    const entry = matchingDownload(catalog, {
      modelId,
      name,
      role: 'audio_frontend',
      quantization: null,
    });
    return entry?.selected && entry.supported && entry.runtimeReady
      ? [{ modelId, name, role: 'audio_frontend' as const, quantization: null }]
      : [];
  });
}

/** Retry transfers automatically without a tight network loop. */
export function audioSetupRetryDelay(failures: number): number {
  return [30_000, 120_000, 600_000][Math.min(Math.max(failures - 1, 0), 2)];
}

export function downloadPercent(progress: InstallProgress | null): number | null {
  if (
    !progress ||
    !Number.isFinite(progress.totalBytes) ||
    progress.totalBytes <= 0 ||
    !Number.isFinite(progress.completedBytes) ||
    progress.completedBytes < 0
  )
    return null;
  return Math.floor(Math.min(progress.completedBytes / progress.totalBytes, 1) * 1000) / 10;
}
export function downloadBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—';
  const index = Math.min(3, Math.floor(Math.log(Math.max(1, bytes)) / Math.log(1024)));
  return `${(bytes / 1024 ** index).toLocaleString(undefined, { maximumFractionDigits: index ? 1 : 0 })} ${['B', 'KiB', 'MiB', 'GiB'][index]}`;
}
export function downloadKey(target: DownloadTarget | ModelCatalogInstallation): string {
  return `${target.role}:${target.modelId}:${target.quantization ?? 'default'}`;
}
export function downloadTarget(config: AppConfig, role: ModelProfileRole): DownloadTarget {
  const speech = role === 'asr';
  const profile = speech ? config.models.customAsr : config.models.customRewriter;
  return {
    role: speech ? 'speech_recognition' : 'text_reconstruction',
    modelId: profile?.id ?? (speech ? config.models.asr : config.models.rewriter),
    name:
      profile?.name ??
      (speech ? ASR_MODELS[config.models.asr] : CLEANUP_MODELS[config.models.rewriter].name),
    quantization: speech || profile ? null : config.rewriteQuantization,
  };
}
export function matchingDownload(
  catalog: ModelCatalogInstallation[],
  target: DownloadTarget,
): ModelCatalogInstallation | undefined {
  return catalog.find((entry) => downloadKey(entry) === downloadKey(target));
}

/** A dropdown preview can inspect a saved catalog package without activating it.
 * A same-ID imported declaration must not reuse old status. */
export function knownDownloadSelection(
  config: AppConfig,
  saved: AppConfig | null,
  role: ModelProfileRole,
): boolean {
  if (!saved) return false;
  if (
    role === 'asr' &&
    (config.computeBackend !== saved.computeBackend || config.language !== saved.language)
  )
    return false;
  const profile = role === 'asr' ? config.models.customAsr : config.models.customRewriter;
  if (!profile) return true;
  const savedProfile = role === 'asr' ? saved.models.customAsr : saved.models.customRewriter;
  if (savedProfile?.id === profile.id) return sameModelProfile(profile, savedProfile);
  return Boolean(presetForProfile(profile));
}

export interface DownloadOptionStatus {
  label: string;
  tone: 'ready' | 'muted' | 'warning';
  kind: 'downloaded' | 'missing' | 'busy' | 'warning' | 'unavailable' | 'unknown';
  detail?: string;
}
export function downloadOptionStatus(
  config: AppConfig,
  saved: AppConfig | null,
  role: ModelProfileRole,
  state: ModelDownloadState,
  desktop: boolean,
  allPrecisions = false,
): DownloadOptionStatus {
  const muted = (
    label: string,
    kind: DownloadOptionStatus['kind'] = 'unknown',
  ): DownloadOptionStatus => ({ label, tone: 'muted', kind });
  if (!desktop) return muted('Desktop only', 'unavailable');
  if (!knownDownloadSelection(config, saved, role)) return muted('Save to check');
  if (state.checking) return muted('Checking…', 'busy');
  const target = downloadTarget(config, role);
  const entries = state.catalog.filter(
    (entry) =>
      entry.modelId === target.modelId &&
      entry.role === target.role &&
      (allPrecisions || entry.quantization === target.quantization),
  );
  if (!entries.length) return muted('Status unavailable');
  if (entries.some((entry) => downloadKey(entry) === state.installingKey))
    return muted('Downloading…', 'busy');
  const downloaded = entries.filter((entry) => entry.artifactsInstalled);
  if (downloaded.length) {
    const precisions = allPrecisions
      ? downloaded.flatMap((entry) =>
          entry.quantization ? [entry.quantization.slice(0, 2).toUpperCase()] : [],
        )
      : [];
    return {
      label: precisions.length ? `${precisions.join(' / ')} downloaded` : 'Downloaded',
      tone: 'ready',
      kind: 'downloaded',
      detail: precisions.length ? precisions.join(' / ') : undefined,
    };
  }
  if (entries.some((entry) => entry.ready))
    return { label: 'Ready · update available', tone: 'ready', kind: 'downloaded' };
  if (entries.some((entry) => entry.localFileCount > 0 || entry.removalError))
    return { label: 'Incomplete download', tone: 'warning', kind: 'warning' };
  return entries.every((entry) => !entry.supported)
    ? muted('Unavailable', 'unavailable')
    : muted('Not downloaded', 'missing');
}
