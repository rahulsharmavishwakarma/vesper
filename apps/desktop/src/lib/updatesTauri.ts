// Native backend for the update manager (#66). The only module that talks to
// the updater/process plugins; the state machine stays pure and testable.
import { check as tauriCheck, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { isTauri } from '@tauri-apps/api/core';
import { getEngineDiagnostics } from './api/desktop';
import { UpdateManager, type UpdateBackend, type UpdateInfo } from './updates';

let pending: Update | null = null;

export const updateBackend: UpdateBackend = {
  enabled: isTauri(),

  async check(): Promise<UpdateInfo | null> {
    pending = await tauriCheck();
    if (!pending) return null;
    return {
      version: pending.version,
      notes: pending.body ?? '',
      date: pending.date,
    };
  },

  async download(
    _update: UpdateInfo,
    onProgress: (downloadedBytes: number, totalBytes: number | null) => void,
  ): Promise<void> {
    if (!pending) throw new Error('No checked update is waiting to download.');
    let downloaded = 0;
    await pending.download((event) => {
      if (event.event === 'Started') {
        onProgress(0, event.data.contentLength ?? null);
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength;
        onProgress(downloaded, null);
      }
    });
  },

  async install(_update: UpdateInfo): Promise<void> {
    if (!pending) throw new Error('No downloaded update is waiting to install.');
    await pending.install();
    pending = null;
  },

  async relaunch(): Promise<void> {
    await relaunch();
  },

  async operationState() {
    const diagnostics = await getEngineDiagnostics();
    return diagnostics.activity;
  },
};

export const updateManager = new UpdateManager(updateBackend);

let quietCheckFired = false;

/** Fire the one quiet startup check; safe to call more than once. */
export function requestQuietCheck(): void {
  if (quietCheckFired) return;
  quietCheckFired = true;
  if (isTauri()) void updateManager.check();
}
