const entrances = new Set([
  'section-arrive',
  'home-arrive',
  'metric-arrive',
  'activity-draw',
  'activity-fill',
  'point-arrive',
]);

/** Settle presentation before focus or a click measures an anchored control.
 * Finishing the existing animation avoids replaying it when focus moves away. */
export function finishEntranceAnimations(event: Event): void {
  if (!(event.target instanceof Element)) return;
  const chart = event.target.closest('.activity-chart');
  if (event.type === 'pointerover' && !chart) return;
  const finish = (animations: Animation[]) => {
    for (const animation of animations) {
      if (
        'animationName' in animation &&
        entrances.has(String(animation.animationName)) &&
        animation.playState !== 'finished'
      )
        animation.finish();
    }
  };
  if (chart) finish(chart.getAnimations({ subtree: true }));
  for (
    let node: Element | null = event.target;
    node && !node.matches('.studio-main');
    node = node.parentElement
  ) {
    finish(node.getAnimations());
  }
}
