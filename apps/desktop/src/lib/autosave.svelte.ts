// Debounced autosave with silent retry (issue #91).
//
// Call notify() after every mutation. The write follows after the debounce
// window; a failed write retries quietly with capped backoff while status
// stays 'saving'. Callers gate notify() on validity (invalid edits never
// reach the backend). settled() lets explicit actions wait for a clean
// state before touching saved data.
export type AutosaveStatus = 'idle' | 'saving' | 'saved';

const RETRY_START_MS = 1000;
const RETRY_MAX_MS = 60000;

export class Autosaver {
  status = $state<AutosaveStatus>('idle');
  savedAt = $state<number | null>(null);
  private timer: ReturnType<typeof setTimeout> | undefined;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private retryDelay = RETRY_START_MS;
  private running = false;
  private generation = 0;
  private disposed = false;

  constructor(
    private readonly save: () => Promise<boolean>,
    private readonly delay = 800,
  ) {}

  notify() {
    if (this.disposed) return;
    this.generation += 1;
    if (this.status !== 'saving') this.status = 'saving';
    clearTimeout(this.retryTimer);
    if (!this.running) this.schedule(this.delay);
  }

  /** Mark clean without writing (initial load). */
  noteSaved() {
    if (this.disposed) return;
    this.status = 'saved';
    this.savedAt = Date.now();
  }

  /** Write through immediately, then wait for a clean state. */
  async flush(timeoutMs = 5000): Promise<boolean> {
    if (this.disposed) return false;
    clearTimeout(this.timer);
    clearTimeout(this.retryTimer);
    if (!this.running) await this.run();
    return this.settled(timeoutMs);
  }

  /** Wait until no write is pending; false on timeout or dispose. */
  async settled(timeoutMs = 5000): Promise<boolean> {
    const deadline = Date.now() + timeoutMs;
    while (this.status === 'saving' && !this.disposed && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    return this.status !== 'saving';
  }

  dispose() {
    this.disposed = true;
    clearTimeout(this.timer);
    clearTimeout(this.retryTimer);
  }

  private schedule(ms: number) {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.run(), ms);
  }

  private async run(): Promise<void> {
    if (this.disposed || this.running) return;
    this.running = true;
    try {
      const seen = this.generation;
      const saved = await this.save();
      if (this.disposed) return;
      if (saved && this.generation === seen) {
        this.retryDelay = RETRY_START_MS;
        this.status = 'saved';
        this.savedAt = Date.now();
      } else if (this.generation !== seen) {
        this.schedule(this.delay);
      } else {
        this.scheduleRetry();
      }
    } catch {
      if (!this.disposed) this.scheduleRetry();
    } finally {
      this.running = false;
    }
  }

  private scheduleRetry() {
    clearTimeout(this.retryTimer);
    this.retryTimer = setTimeout(() => {
      if (!this.disposed && !this.running) void this.run();
    }, this.retryDelay);
    this.retryDelay = Math.min(this.retryDelay * 2, RETRY_MAX_MS);
  }
}
