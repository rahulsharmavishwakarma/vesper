// Display continuity only: these held values are not new pitch measurements.
// Missing estimates during unvoiced consonants must not become a new wavelength.
export class PitchDisplay {
  private pitch: number | null = null;
  private observedAt = -Infinity;
  private candidate: { pitch: number; since: number } | null = null;

  reset() {
    this.pitch = null;
    this.observedAt = -Infinity;
    this.candidate = null;
  }

  update(measured: number | null, now: number): number | null {
    if (now - this.observedAt >= 160) this.reset();
    const valid =
      measured !== null && Number.isFinite(measured) && measured >= 60 && measured <= 700;
    if (!valid) {
      this.candidate = null;
      return this.pitch;
    }
    // Confirm a large jump across 60 ms of observed input. A single octave
    // error cannot reshape the display; a sustained real pitch step still can.
    if (this.pitch !== null && Math.abs(Math.log2(measured / this.pitch)) >= 0.75) {
      if (!this.candidate || Math.abs(Math.log2(measured / this.candidate.pitch)) > 0.2) {
        this.candidate = { pitch: measured, since: now };
      }
      if (now - this.candidate.since < 60) return this.pitch;
    }
    this.pitch = measured;
    this.observedAt = now;
    this.candidate = null;
    return this.pitch;
  }
}
