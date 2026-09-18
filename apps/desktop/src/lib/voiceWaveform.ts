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

/** W02 Reed: tapered round strokes. The caller supplies measured/smoothed energy;
 * phase adds motion, never amplitude. Silence stays a quiet row of short strokes. */
export function reedPath(
  amplitude: number,
  cycles: number,
  phase: number,
  compact = false,
): string {
  const level = Number.isFinite(amplitude) ? Math.max(0, Math.min(1, amplitude)) : 0;
  const frequency = Number.isFinite(cycles) ? Math.max(2, Math.min(8, cycles)) : 3;
  const time = Number.isFinite(phase) ? phase : 0;
  const count = compact ? 11 : 45;
  return Array.from({ length: count }, (_, index) => {
    const x = index / (count - 1);
    const envelope = Math.sin(Math.PI * x) ** 1.55;
    // Every lobe follows the supplied pitch. Unrelated decorative oscillators
    // obscure wavelength changes even when microphone telemetry is correct.
    const signal = Math.sin(x * frequency * 2 * Math.PI - time);
    const height = 2 + level * 80 * envelope * (0.32 + 0.68 * Math.abs(signal));
    return `M ${(2 + x * 316).toFixed(2)} ${(50 - height / 2).toFixed(2)} V ${(50 + height / 2).toFixed(2)}`;
  }).join(' ');
}
