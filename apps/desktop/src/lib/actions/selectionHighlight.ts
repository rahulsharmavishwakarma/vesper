/** B04/H02: one backing, measured from stable button geometry. Switches run
 * FLIP entirely on transform: the size vars snap to the target while a
 * compensating scale starts the backing at the visible box, then releases to
 * identity. An interrupted glide retargets from its live ::before matrix, so
 * rapid changes never jump and no interaction transition animates layout. */
export function selectionHighlight(node: HTMLElement, _selection: unknown) {
  return highlight(node, 'button[aria-pressed="true"]');
}

export function optionHighlight(node: HTMLElement, _active: number) {
  return highlight(node, 'button.is-active');
}

type Box = { x: number; y: number; width: number; height: number };

function highlight(node: HTMLElement, selector: string) {
  let disposed = false;
  let frame = 0;
  let box: Box | undefined;
  node.classList.add('sliding-selection');
  const target = (): Box | undefined => {
    const button = node.querySelector<HTMLElement>(selector);
    if (!button || button.offsetWidth <= 0) return undefined;
    return {
      x: button.offsetLeft,
      y: button.offsetTop,
      width: button.offsetWidth,
      height: button.offsetHeight,
    };
  };
  // The ::before backing is the only painted box: it sits at the host origin
  // with the snapped size vars, so its live matrix IS the visible geometry
  // mid-glide and a new switch starts exactly where the eye is.
  const rendered = (): Box | undefined => {
    if (!box) return undefined;
    const matrix = new DOMMatrix(getComputedStyle(node, '::before').transform);
    const width = Number.parseFloat(node.style.getPropertyValue('--selection-width'));
    const height = Number.parseFloat(node.style.getPropertyValue('--selection-height'));
    if (!width || !height) return box;
    return {
      x: matrix.e,
      y: matrix.f,
      width: width * matrix.a,
      height: height * matrix.d,
    };
  };
  const paint = (next: Box) => {
    node.style.setProperty('--selection-x', `${next.x}px`);
    node.style.setProperty('--selection-y', `${next.y}px`);
    node.style.setProperty('--selection-width', `${next.width}px`);
    node.style.setProperty('--selection-height', `${next.height}px`);
  };
  function measure(animate = true) {
    if (disposed) return;
    cancelAnimationFrame(frame);
    const next = target();
    if (!next) {
      delete node.dataset.hasSelection;
      box = undefined;
      return;
    }
    node.dataset.hasSelection = 'true';
    const from = animate ? (rendered() ?? next) : next;
    // Invert with transitions parked, then release the scale on the next
    // frame so the backing glides from the visible box to the target.
    node.removeAttribute('data-motion-ready');
    paint(next);
    node.style.setProperty('--selection-sx', `${from.width / next.width}`);
    node.style.setProperty('--selection-sy', `${from.height / next.height}`);
    // Origin is top-left, so the from corner lands exactly with no adjustment.
    node.style.setProperty('--selection-x', `${from.x}px`);
    node.style.setProperty('--selection-y', `${from.y}px`);
    void node.offsetWidth;
    node.dataset.motionReady = 'true';
    box = next;
    if (!animate) {
      node.style.setProperty('--selection-sx', '1');
      node.style.setProperty('--selection-sy', '1');
      paint(next);
      return;
    }
    frame = requestAnimationFrame(() => {
      if (disposed) return;
      paint(next);
      node.style.setProperty('--selection-sx', '1');
      node.style.setProperty('--selection-sy', '1');
    });
  }
  const observer = new ResizeObserver(() => measure(false));
  observer.observe(node);
  node.querySelectorAll('button').forEach((button) => observer.observe(button));
  measure(false);
  return {
    update(_selection: unknown) {
      queueMicrotask(() => measure());
    },
    destroy() {
      disposed = true;
      cancelAnimationFrame(frame);
      observer.disconnect();
    },
  };
}
