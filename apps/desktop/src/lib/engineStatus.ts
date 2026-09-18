export interface ResourceSample {
  sampledAtMs: number;
  cpuPercent: number | null;
  memoryBytes: number | null;
  logicalCpus: number;
  status: 'ready' | 'warming' | 'unavailable';
}
export interface JobTimings {
  operationId: string;
  asrModel: string;
  cleanupModel: string | null;
  speechMode: 'offline' | 'finalization' | 'fallback';
  outcome: 'running' | 'completed' | 'cancelled' | 'error';
  activeStage: 'transcribe' | 'cleanup' | 'save' | null;
  transcribeMs: number | null;
  cleanupMs: number | null;
  saveMs: number | null;
}
export interface EngineDiagnostics {
  resources: ResourceSample;
  activity: { busy: boolean; stage: string };
  timings: JobTimings | null;
}
export function validSample(sample: ResourceSample, now = Date.now()): ResourceSample {
  if (
    !Number.isFinite(sample.sampledAtMs) ||
    now - sample.sampledAtMs > 3000 ||
    sample.sampledAtMs > now + 1000
  ) {
    throw new Error('Live readings are stale. Refresh to try again.');
  }
  return {
    ...sample,
    cpuPercent:
      sample.status === 'ready' &&
      sample.cpuPercent !== null &&
      Number.isFinite(sample.cpuPercent) &&
      sample.cpuPercent >= 0 &&
      sample.cpuPercent <= 100
        ? sample.cpuPercent
        : null,
    memoryBytes:
      sample.memoryBytes !== null && Number.isFinite(sample.memoryBytes) && sample.memoryBytes > 0
        ? sample.memoryBytes
        : null,
  };
}
export function appendSample(history: ResourceSample[], sample: ResourceSample): ResourceSample[] {
  const last = history.at(-1);
  if (last?.sampledAtMs === sample.sampledAtMs) return history;
  const previous = last && last.sampledAtMs > sample.sampledAtMs ? [] : history;
  return [
    ...previous.filter((point) => sample.sampledAtMs - point.sampledAtMs < 60_000),
    sample,
  ].slice(-60);
}
export function memoryScale(history: ResourceSample[]): number {
  const peak = Math.max(2 ** 28, ...history.map((point) => point.memoryBytes ?? 0));
  return 2 ** Math.ceil(Math.log2(peak * 1.1));
}
export function resourcePath(
  history: ResourceSample[],
  key: 'cpuPercent' | 'memoryBytes',
  scale: number,
): string {
  const end = history.at(-1)?.sampledAtMs;
  if (end === undefined) return '';
  let previous: number | null = null;
  return history
    .map((point) => {
      const value = point[key];
      if (value === null) {
        previous = null;
        return '';
      }
      const connected = previous !== null && point.sampledAtMs - previous <= 2500;
      previous = point.sampledAtMs;
      return `${connected ? 'L' : 'M'}${(((point.sampledAtMs - end + 60_000) / 60_000) * 400).toFixed(1)},${(76 - Math.min(value / scale, 1) * 68).toFixed(1)}`;
    })
    .join(' ');
}

/** B06 bars occupy their actual positions in the 60-second history. Missing
 * readings leave gaps; the entrance animation never changes displayed values. */
export function resourceBars(
  history: ResourceSample[],
  key: 'cpuPercent' | 'memoryBytes',
  scale: number,
) {
  const end = history.at(-1)?.sampledAtMs;
  if (end === undefined || !Number.isFinite(scale) || scale <= 0) return [];
  return history.flatMap((point) => {
    const value = point[key];
    if (value === null || !Number.isFinite(value) || value < 0) return [];
    const height = Math.min(value / scale, 1) * 68;
    return [
      {
        time: point.sampledAtMs,
        x: Math.max(0, Math.min(396, ((point.sampledAtMs - end + 60_000) / 60_000) * 400 - 4)),
        y: 76 - height,
        height,
      },
    ];
  });
}
export function durationLabel(ms: number | null | undefined): string {
  return ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0
    ? '—'
    : ms < 1000
      ? `${Math.round(ms)} ms`
      : `${(ms / 1000).toFixed(1)} s`;
}
export const activityLabels: Record<string, string> = {
  idle: 'Ready to listen',
  recording: 'Recording',
  recording_complete: 'Finishing recording',
  transcribing: 'Transcribing',
  rewriting: 'Polishing text',
  installing: 'Installing models',
  removing: 'Removing model files',
  cancelling: 'Cancelling…',
  cancelled: 'Cancelled',
  error: 'Operation failed',
};

/** One request and one timer at most. Late responses cannot revive paused/disposed views. */
export function createResourcePoller<T>(
  read: () => Promise<T>,
  accept: (value: T) => void,
  fail: (error: unknown) => void,
) {
  let enabled = false,
    disposed = false,
    pending = false,
    generation = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function clear() {
    clearTimeout(timer);
    timer = undefined;
  }
  async function poll() {
    if (!enabled || disposed || pending) return;
    clear();
    pending = true;
    const request = generation;
    let expired = false;
    timer = setTimeout(() => {
      timer = undefined;
      expired = true;
      if (enabled && !disposed && request === generation)
        fail(new Error('Live readings are taking too long. Waiting for the current request.'));
    }, 3000);
    try {
      const value = await read();
      if (enabled && !disposed && request === generation && !expired) accept(value);
    } catch (error) {
      if (enabled && !disposed && request === generation) fail(error);
    } finally {
      pending = false;
      clear();
      if (enabled && !disposed)
        timer = setTimeout(() => void poll(), request === generation ? 1000 : 0);
    }
  }
  return {
    setEnabled(next: boolean) {
      if (next === enabled || disposed) return;
      enabled = next;
      generation++;
      clear();
      if (next) {
        if (pending)
          timer = setTimeout(() => {
            timer = undefined;
            fail(new Error('Live readings are taking too long. Waiting for the current request.'));
          }, 3000);
        else void poll();
      }
    },
    refresh() {
      if (enabled && !pending) {
        clear();
        void poll();
      }
    },
    dispose() {
      disposed = true;
      enabled = false;
      generation++;
      clear();
    },
  };
}
