/** A readable visual scale, not an oscilloscope time axis. */
export function waveformEnergy(level: number): number {
  if (!Number.isFinite(level) || level <= 0) return 0;
  const dbScale = Math.max(0, Math.min(1, (20 * Math.log10(level) + 60) / 48));
  // A soft low-end response keeps faint ambient input small without a speech
  // gate that would discard whispers and unvoiced consonants.
  return dbScale * dbScale;
}

/** Unvoiced sound has no new wavelength. Retain visual spacing until silence,
 * without promoting it to a fresh pitch measurement. */
export function waveformCycles(current: number, measured: number | null, silent: boolean): number {
  if (silent) return 3;
  return measured === null || !Number.isFinite(measured) || measured < 60 || measured > 700
    ? current
    : pitchCycles(measured);
}

/** A readable visual scale, not an oscilloscope time axis. */
export function pitchCycles(pitchHz: number | null): number {
  if (pitchHz === null || !Number.isFinite(pitchHz) || pitchHz < 60 || pitchHz > 700) return 3;
  return 2 + 1.6 * Math.log2(pitchHz / 60);
}

/** Frame-rate independent signal following; settle exactly instead of redrawing
 * subpixel tails forever. These constants are UI smoothing, not audio latency. */
export function followSignal(
  current: number,
  target: number,
  elapsedMs: number,
  timeConstantMs: number,
): number {
  const next =
    current + (target - current) * (1 - Math.exp(-Math.max(0, elapsedMs) / timeConstantMs));
  return Math.abs(target - next) < 0.0005 ? target : next;
}

export function followLevel(current: number, target: number, elapsedMs: number): number {
  return followSignal(current, target, elapsedMs, target > current ? 35 : 70);
}

function reedShape(cycles: number, phase: number, count: number): number[] {
  const frequency = Number.isFinite(cycles) ? Math.max(2, Math.min(8, cycles)) : 3;
  const time = Number.isFinite(phase) ? phase : 0;
  return Array.from({ length: count }, (_, index) => {
    const x = index / (count - 1);
    return (
      Math.sin(Math.PI * x) ** 1.55 *
      (0.32 + 0.68 * Math.abs(Math.sin(x * frequency * 2 * Math.PI - time)))
    );
  });
}

function shapePath(amplitude: number, shape: number[]): string {
  const level = Number.isFinite(amplitude) ? Math.max(0, Math.min(1, amplitude)) : 0;
  return shape
    .map((factor, index) => {
      const x = index / (shape.length - 1);
      const height = 2 + level * 80 * factor;
      return `M ${(2 + x * 316).toFixed(2)} ${(50 - height / 2).toFixed(2)} V ${(50 + height / 2).toFixed(2)}`;
    })
    .join(' ');
}

/** Morph the existing stroke heights, not cycles through a nonlinear sine.
 * Each stroke stays between its old and new pitch shapes, with no travelling
 * peaks or overshoot. This holds visual state, never measured pitch. */
export class ReedMotion {
  private shape: number[];
  constructor(compact = false) {
    this.shape = reedShape(3, 0, compact ? 11 : 45);
  }
  path(amplitude: number, cycles: number, elapsedMs: number): string {
    const target = reedShape(cycles, 0, this.shape.length);
    this.shape = target.map((factor, index) =>
      amplitude === 0 ? factor : followSignal(this.shape[index], factor, elapsedMs, 35),
    );
    return shapePath(amplitude, this.shape);
  }
}

/** W02 Reed: tapered round strokes. The caller supplies measured/smoothed energy;
 * phase adds motion, never amplitude. Silence stays a quiet row of short strokes. */
export function reedPath(
  amplitude: number,
  cycles: number,
  phase: number,
  compact = false,
): string {
  return shapePath(amplitude, reedShape(cycles, phase, compact ? 11 : 45));
}
