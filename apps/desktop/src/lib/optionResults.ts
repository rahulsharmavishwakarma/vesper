/** A ring gauge on a picker row: a measured or published result for the
 * chosen language and this Mac. `score` fills the ring from 0 to 100; `null`
 * draws a dashed, empty ring for a result that does not exist yet. */
export interface OptionGauge {
  kind: 'accuracy' | 'speed' | 'memory' | 'instructions' | 'languages';
  icon: 'target' | 'models' | 'memory' | 'shield' | 'globe';
  score: number | null;
  text: string;
  /** Full sentence for the tooltip and the screen-reader label. */
  title: string;
  tone?: 'low' | 'bad';
}
