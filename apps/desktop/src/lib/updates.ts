// Update check/download state machine (#66). Pure and deterministic: all
// native access is injected so the transitions are unit-testable and the
// component layer only renders state. The updater is inert until a public key
// and endpoint are enrolled (#65): checks then fail into a recoverable error
// state instead of pretending to work.

export interface UpdateInfo {
  version: string;
  notes: string;
  date?: string;
}

export interface BusyState {
  busy: boolean;
  stage: string;
}

/** Native surface, implemented once with the Tauri plugins. */
export interface UpdateBackend {
  enabled: boolean;
  /** `includePrereleases` also checks the beta/pre-release feed (#277). */
  check(options: { includePrereleases: boolean }): Promise<UpdateInfo | null>;
  download(
    update: UpdateInfo,
    onProgress: (downloadedBytes: number, totalBytes: number | null) => void,
  ): Promise<void>;
  install(update: UpdateInfo): Promise<void>;
  relaunch(): Promise<void>;
  operationState(): Promise<BusyState>;
}

export type UpdateStatus =
  'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'ready' | 'error';

export interface UpdateState {
  status: UpdateStatus;
  available: UpdateInfo | null;
  downloadedBytes: number;
  totalBytes: number | null;
  lastCheckedAt: string | null;
  errorPhase: 'check' | 'download' | 'install' | 'restart' | null;
  errorMessage: string | null;
  blockedReason: string | null;
  /** Check at startup and when About opens (default on). */
  autoCheck: boolean;
  /** Download and install a found release without a click (default off). */
  autoInstall: boolean;
  /** Also offer beta and pre-release builds (default off: stable only). */
  includePrereleases: boolean;
}

export interface UpdatePreferences {
  autoCheck: boolean;
  autoInstall: boolean;
  includePrereleases: boolean;
}

/** Where the two preferences persist; injected so the manager stays pure. */
export interface UpdatePreferenceStore {
  load(): Partial<UpdatePreferences> | null;
  save(preferences: UpdatePreferences): void;
}

export const DEFAULT_UPDATE_PREFERENCES: UpdatePreferences = {
  autoCheck: true,
  autoInstall: false,
  includePrereleases: false,
};

/** Installing automatically is meaningless without checking automatically. */
export function normalizeUpdatePreferences(
  value: Partial<UpdatePreferences> | null | undefined,
): UpdatePreferences {
  const autoCheck =
    typeof value?.autoCheck === 'boolean' ? value.autoCheck : DEFAULT_UPDATE_PREFERENCES.autoCheck;
  const autoInstall =
    typeof value?.autoInstall === 'boolean'
      ? value.autoInstall
      : DEFAULT_UPDATE_PREFERENCES.autoInstall;
  const includePrereleases =
    typeof value?.includePrereleases === 'boolean'
      ? value.includePrereleases
      : DEFAULT_UPDATE_PREFERENCES.includePrereleases;
  return { autoCheck, autoInstall: autoCheck && autoInstall, includePrereleases };
}

function versionParts(version: string): { core: number[]; pre: string[] } {
  const bare = version.trim().replace(/^v/, '').split('+')[0];
  const dash = bare.indexOf('-');
  const core = (dash < 0 ? bare : bare.slice(0, dash)).split('.').map((part) => Number(part) || 0);
  const pre = dash < 0 ? [] : bare.slice(dash + 1).split('.');
  return { core, pre };
}

/** SemVer precedence (spec item 11): a pre-release sorts below its release. */
export function compareVersions(a: string, b: string): number {
  const left = versionParts(a);
  const right = versionParts(b);
  for (let index = 0; index < Math.max(left.core.length, right.core.length); index += 1) {
    const difference = (left.core[index] ?? 0) - (right.core[index] ?? 0);
    if (difference !== 0) return difference < 0 ? -1 : 1;
  }
  if (!left.pre.length || !right.pre.length) {
    return left.pre.length === right.pre.length ? 0 : left.pre.length ? -1 : 1;
  }
  for (let index = 0; index < Math.max(left.pre.length, right.pre.length); index += 1) {
    const x = left.pre[index];
    const y = right.pre[index];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xNumeric = /^\d+$/.test(x);
    const yNumeric = /^\d+$/.test(y);
    if (xNumeric && yNumeric) {
      if (Number(x) !== Number(y)) return Number(x) < Number(y) ? -1 : 1;
    } else if (xNumeric !== yNumeric) {
      return xNumeric ? -1 : 1;
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}

export interface CheckOutcome<T> {
  value: T | null;
  error: unknown;
}

/**
 * Combine the stable feed with the optional pre-release feed. Each feed only
 * reports a version newer than the installed one, so the higher of the two wins
 * (a tester moves to a stable release once it overtakes the last rc). Only when
 * every consulted feed failed is the check an error; a failing pre-release feed
 * never hides a working stable answer.
 */
export function selectCandidate<T extends { version: string }>(
  stable: CheckOutcome<T>,
  prerelease: CheckOutcome<T> | null,
): { chosen: T | null; error: unknown } {
  const consulted = prerelease ? [stable, prerelease] : [stable];
  if (consulted.every((outcome) => outcome.error != null)) {
    return { chosen: null, error: stable.error };
  }
  let chosen: T | null = null;
  for (const outcome of consulted) {
    const candidate = outcome.error == null ? outcome.value : null;
    if (candidate && (!chosen || compareVersions(candidate.version, chosen.version) > 0)) {
      chosen = candidate;
    }
  }
  return { chosen, error: null };
}

export function initialUpdateState(
  preferences: UpdatePreferences = DEFAULT_UPDATE_PREFERENCES,
): UpdateState {
  return {
    status: 'idle',
    available: null,
    downloadedBytes: 0,
    totalBytes: null,
    lastCheckedAt: null,
    errorPhase: null,
    errorMessage: null,
    blockedReason: null,
    autoCheck: preferences.autoCheck,
    autoInstall: preferences.autoInstall,
    includePrereleases: preferences.includePrereleases,
  };
}

/** Plain-language label for the active operation blocking install/restart. */
export function busyLabel(stage: string): string {
  switch (stage) {
    case 'recording':
      return 'Recording is in progress';
    case 'transcribing':
      return 'Transcription is in progress';
    case 'rewriting':
      return 'Writing cleanup is in progress';
    case 'installing':
      return 'A model download is in progress';
    case 'removing':
      return 'A model removal is in progress';
    default:
      return 'Another operation is in progress';
  }
}

export class UpdateManager {
  state: UpdateState;
  private listeners = new Set<(state: UpdateState) => void>();
  private inFlight = false;
  private backend: UpdateBackend;
  private store: UpdatePreferenceStore | null;

  constructor(backend: UpdateBackend, store: UpdatePreferenceStore | null = null) {
    this.backend = backend;
    this.store = store;
    this.state = initialUpdateState(normalizeUpdatePreferences(store?.load()));
  }

  /** Change a preference; persists, notifies, and installs a waiting release when enabled. */
  setPreferences(change: Partial<UpdatePreferences>): void {
    const channelBefore = this.state.includePrereleases;
    const next = normalizeUpdatePreferences({
      autoCheck: this.state.autoCheck,
      autoInstall: this.state.autoInstall,
      includePrereleases: this.state.includePrereleases,
      ...change,
    });
    this.store?.save(next);
    this.set(next);
    if (next.includePrereleases !== channelBefore) {
      // A stale answer from the other channel must not stay on offer. Leave a
      // download or installed update alone; the choice applies to the next check.
      const settled = ['idle', 'current', 'available', 'error'].includes(this.state.status);
      if (settled && !this.inFlight) {
        this.set({
          status: 'idle',
          available: null,
          errorPhase: null,
          errorMessage: null,
          blockedReason: null,
        });
        if (next.autoCheck) void this.check();
      }
      return;
    }
    if (next.autoInstall && this.state.status === 'available') void this.downloadAndInstall();
  }

  /** The quiet startup / first-view check, honoring the automatic-check preference. */
  async automaticCheck(): Promise<void> {
    if (!this.state.autoCheck) return;
    await this.check();
  }

  subscribe(listener: (state: UpdateState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private set(partial: Partial<UpdateState>) {
    this.state = { ...this.state, ...partial };
    for (const listener of this.listeners) listener(this.state);
  }

  /** Quiet check (startup): same transitions, no UI of its own. */
  async check(): Promise<void> {
    if (this.inFlight) return;
    if (!this.backend.enabled) {
      this.set({
        status: 'error',
        errorPhase: 'check',
        errorMessage: 'Automatic updates are not configured in this build.',
        blockedReason: null,
      });
      return;
    }
    this.inFlight = true;
    let install = false;
    this.set({ status: 'checking', errorPhase: null, errorMessage: null, blockedReason: null });
    try {
      const update = await this.backend.check({
        includePrereleases: this.state.includePrereleases,
      });
      const checkedAt = new Date().toISOString();
      if (update) {
        this.set({ status: 'available', available: update, lastCheckedAt: checkedAt });
        install = this.state.autoInstall;
      } else {
        this.set({ status: 'current', available: null, lastCheckedAt: checkedAt });
      }
    } catch (error) {
      this.set({
        status: 'error',
        errorPhase: 'check',
        errorMessage: `Could not check for updates. ${describeError(error)}`,
      });
    } finally {
      this.inFlight = false;
    }
    if (install) await this.downloadAndInstall();
  }

  async downloadAndInstall(): Promise<void> {
    const update = this.state.available;
    if (!update || this.inFlight) return;
    this.inFlight = true;
    this.set({
      status: 'downloading',
      downloadedBytes: 0,
      totalBytes: null,
      errorPhase: null,
      errorMessage: null,
      blockedReason: null,
    });
    try {
      const busy = await this.backend.operationState();
      if (busy.busy) {
        this.set({
          status: 'available',
          blockedReason: `${busyLabel(busy.stage)} — finish it before installing.`,
        });
        return;
      }
      await this.backend.download(update, (downloaded, total) => {
        this.set({ downloadedBytes: downloaded, totalBytes: total ?? this.state.totalBytes });
      });
      // Install replaces the app bundle immediately; guard again against an
      // operation that started while downloading.
      const busyAgain = await this.backend.operationState();
      if (busyAgain.busy) {
        this.set({
          status: 'available',
          blockedReason: `${busyLabel(busyAgain.stage)} — finish it before installing.`,
        });
        return;
      }
      await this.backend.install(update);
      this.set({ status: 'ready' });
    } catch (error) {
      const phase =
        this.state.status === 'downloading' && this.state.downloadedBytes === 0
          ? 'download'
          : 'install';
      this.set({
        status: 'error',
        errorPhase: phase,
        errorMessage: `The update failed to ${phase === 'download' ? 'download' : 'install'}. ${describeError(error)}`,
      });
    } finally {
      this.inFlight = false;
    }
  }

  async restart(): Promise<void> {
    if (this.state.status !== 'ready' || this.inFlight) return;
    this.inFlight = true;
    try {
      const busy = await this.backend.operationState();
      if (busy.busy) {
        this.set({ blockedReason: `${busyLabel(busy.stage)} — finish it before restarting.` });
        return;
      }
      await this.backend.relaunch();
    } catch (error) {
      this.set({
        status: 'error',
        errorPhase: 'restart',
        errorMessage: `Could not restart to finish the update. ${describeError(error)}`,
      });
    } finally {
      this.inFlight = false;
    }
  }
}

function describeError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  // Tauri updater errors surface as opaque strings; give plain recovery text
  // without claiming a cause we cannot verify.
  if (/offline|network|fetch|connect/i.test(message)) return 'Check your connection and try again.';
  if (/signature|verify/i.test(message))
    return 'The download could not be verified, so nothing was installed.';
  return 'You can try again.';
}
