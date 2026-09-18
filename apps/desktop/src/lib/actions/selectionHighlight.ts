/** B04/H02: one backing, measured from stable button geometry. CSS retargets
 * an interrupted slide from its visible position; resizing settles immediately. */
export function selectionHighlight(node: HTMLElement, _selection: unknown) {
  return highlight(node, 'button[aria-pressed="true"]');
}

export function optionHighlight(node: HTMLElement, _active: number) {
  return highlight(node, 'button.is-active');
}

function highlight(node: HTMLElement, selector: string) {
  let disposed = false;
  node.classList.add('sliding-selection');
  function measure(animate = true) {
    if (disposed) return;
    const button = node.querySelector<HTMLElement>(selector);
    if (!animate) node.removeAttribute('data-motion-ready');
    if (button && button.offsetWidth > 0) {
      node.style.setProperty('--selection-x', `${button.offsetLeft}px`);
      node.style.setProperty('--selection-y', `${button.offsetTop}px`);
      node.style.setProperty('--selection-width', `${button.offsetWidth}px`);
      node.style.setProperty('--selection-height', `${button.offsetHeight}px`);
      node.dataset.hasSelection = 'true';
    } else {
      delete node.dataset.hasSelection;
    }
    if (!animate) {
      // Commit layout without transitioning across rows or a resized container.
      void node.offsetWidth;
      node.dataset.motionReady = 'true';
    }
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
      observer.disconnect();
    },
  };
}
