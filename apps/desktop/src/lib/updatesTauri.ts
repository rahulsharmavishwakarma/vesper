// Native backend for the update manager (#66). The only module that talks to
// the updater/process plugins; the state machine stays pure and testable.
import { check as tauriCheck, Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { getVersion } from '@tauri-apps/api/app';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getEngineDiagnostics } from './api/desktop';
import {
  selectCandidate,
  UpdateManager,
  type CheckOutcome,
  type UpdateBackend,
  type UpdateInfo,
  type UpdatePreferenceStore,
} from './updates';

let pending: Update | null = null;

async function settle(run: () => Promise<Update | null>): Promise<CheckOutcome<Update>> {
  try {
    return { value: await run(), error: null };
  } catch (error) {
    return { value: null, error };
  }
}

/** The pre-release feed, checked by a main-window-only Rust command (#277). */
async function checkPrerelease(): Promise<Update | null> {
  const metadata = await invoke<ConstructorParameters<typeof Update>[0] | null>(
    'check_prerelease_update',
  );
  return metadata ? new Update(metadata) : null;
}

// Nightly builds follow their own feed and key; they never take this channel.
let nightlyBuild: boolean | null = null;
async function isNightlyBuild(): Promise<boolean> {
  if (nightlyBuild === null) nightlyBuild = (await getVersion()).includes('-nightly.');
  return nightlyBuild;
}

export const updateBackend: UpdateBackend = {
  enabled: isTauri(),

  async check({ includePrereleases }): Promise<UpdateInfo | null> {
    const stable = await settle(() => tauriCheck());
    const wantsPrerelease = includePrereleases && !(await isNightlyBuild());
    const prerelease = wantsPrerelease ? await settle(checkPrerelease) : null;
    const { chosen, error } = selectCandidate(stable, prerelease);
    if (error) throw error;
    // Free the update that lost, best effort; the plugin keeps it in a table.
    for (const outcome of [stable, prerelease]) {
      if (outcome?.value && outcome.value !== chosen) void outcome.value.close().catch(() => {});
    }
    pending = chosen;
    if (!chosen) return null;
    return {
      version: chosen.version,
      notes: chosen.body ?? '',
      date: chosen.date,
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

const PREFERENCES_KEY = 'vesper.updatePreferences';

// A small frontend preference, kept beside the manager rather than in the
// settings file; storage can be unavailable, so every access is guarded.
const preferenceStore: UpdatePreferenceStore = {
  load() {
    try {
      const raw = localStorage.getItem(PREFERENCES_KEY);
      return raw ? (JSON.parse(raw) as Record<string, unknown>) : null;
    } catch {
      return null;
    }
  },
  save(preferences) {
    try {
      localStorage.setItem(PREFERENCES_KEY, JSON.stringify(preferences));
    } catch {
      // Not persisted this time; the in-memory choice still applies.
    }
  },
};

export const updateManager = new UpdateManager(updateBackend, preferenceStore);

let quietCheckFired = false;

/** Fire the one quiet startup check; safe to call more than once. */
export function requestQuietCheck(): void {
  if (quietCheckFired) return;
  quietCheckFired = true;
  if (isTauri()) void updateManager.automaticCheck();
}
