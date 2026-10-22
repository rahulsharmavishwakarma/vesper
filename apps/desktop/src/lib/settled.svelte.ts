/** Hides brief in-between states. A value that `isPassing` marks (such as
 * "Loading preferences…") is shown only after it has lasted `delayMs`; any
 * other value shows at once and cancels a pending one. Most launches finish
 * loading well inside the delay, so the text never flashes. Create it while a
 * component initialises: it owns an effect. */
export class Settled<T> {
  #shown = $state<T | null>(null);

  constructor(read: () => T, isPassing: (value: T) => boolean, delayMs = 300) {
    $effect(() => {
      const value = read();
      if (!isPassing(value)) {
        this.#shown = value;
        return;
      }
      const timer = setTimeout(() => (this.#shown = value), delayMs);
      return () => clearTimeout(timer);
    });
  }

  /** The value to show, or null while a passing state has not yet lasted. */
  get value(): T | null {
    return this.#shown;
  }
}
