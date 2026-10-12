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
  check(): Promise<UpdateInfo | null>;
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
}

export function initialUpdateState(): UpdateState {
  return {
    status: 'idle',
    available: null,
    downloadedBytes: 0,
    totalBytes: null,
    lastCheckedAt: null,
    errorPhase: null,
    errorMessage: null,
    blockedReason: null,
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
  state: UpdateState = initialUpdateState();
  private listeners = new Set<(state: UpdateState) => void>();
  private inFlight = false;
  private backend: UpdateBackend;

  constructor(backend: UpdateBackend) {
    this.backend = backend;
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
    this.set({ status: 'checking', errorPhase: null, errorMessage: null, blockedReason: null });
    try {
      const update = await this.backend.check();
      const checkedAt = new Date().toISOString();
      if (update) {
        this.set({ status: 'available', available: update, lastCheckedAt: checkedAt });
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
  return 'You can try again, or install the update manually from the download page.';
}
