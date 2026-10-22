import type { InstallProgress, RuntimeStatus } from './api/runtime.ts';
import { downloadKey, downloadPercent, type ModelDownloadState } from './modelDownloads.ts';

/** What the sidebar shows while something runs, from any page. */
export interface SidebarActivity {
  label: string;
  /** 0 to 100 when the work reports progress, otherwise null. */
  percent: number | null;
  /** The page where the work can be followed or cancelled. */
  view: 'dictate' | 'models';
  /** Recording is live input; everything else is background work. */
  live: boolean;
}

/** The background model download added with the Neural Engine work (#301).
 * Read defensively so the sidebar is ready before that field exists. */
export interface BackgroundDownload {
  running: boolean;
  progress: InstallProgress | null;
  error?: string | null;
}

const STAGES: Record<string, { label: string; view: SidebarActivity['view'] }> = {
  recording: { label: 'Recording', view: 'dictate' },
  recording_complete: { label: 'Recording finished', view: 'dictate' },
  transcribing: { label: 'Transcribing', view: 'dictate' },
  identifying_speakers: { label: 'Identifying speakers', view: 'dictate' },
  rewriting: { label: 'Polishing text', view: 'dictate' },
  installing: { label: 'Installing models', view: 'models' },
  removing: { label: 'Removing model files', view: 'models' },
  cancelling: { label: 'Cancelling', view: 'dictate' },
};

export function sidebarActivity(
  runtime: RuntimeStatus | null,
  downloads: ModelDownloadState,
  background: BackgroundDownload | null = null,
): SidebarActivity | null {
  if (downloads.installing) {
    const entry = downloads.catalog.find((item) => downloadKey(item) === downloads.installingKey);
    const name = entry?.name ?? 'model';
    const phase = downloads.details?.phase;
    return {
      label: downloads.cancelling
        ? 'Cancelling download'
        : phase === 'checking' || phase === 'verifying'
          ? `Verifying ${name}`
          : `Downloading ${name}`,
      percent: downloadPercent(downloads.details),
      view: 'models',
      live: false,
    };
  }
  if (runtime?.busy) {
    const stage = STAGES[runtime.stage];
    return {
      label: stage?.label ?? 'Working',
      percent:
        runtime.stage === 'installing' ? downloadPercent(runtime.download?.progress ?? null) : null,
      view: stage?.view ?? 'dictate',
      live: runtime.stage === 'recording',
    };
  }
  if (background?.running)
    return {
      label: 'Preparing other languages',
      percent: downloadPercent(background.progress),
      view: 'models',
      live: false,
    };
  return null;
}
