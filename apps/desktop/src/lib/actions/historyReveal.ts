import { slide } from 'svelte/transition';
import { cubicOut } from 'svelte/easing';

/** B05: expand in place, without moving the row heading or delaying controls. */
export function historyReveal(node: HTMLElement) {
  return slide(node, {
    duration: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 260,
    easing: cubicOut,
  });
}

export function revealAccessibility(node: HTMLElement) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  const settle = () => {
    if (preference.matches) node.getAnimations().forEach((animation) => animation.finish());
  };
  const opening = () => {
    node.inert = false;
    node.removeAttribute('aria-hidden');
  };
  const closing = () => {
    node.inert = true;
    node.setAttribute('aria-hidden', 'true');
  };
  preference.addEventListener('change', settle);
  node.addEventListener('introstart', opening);
  node.addEventListener('outrostart', closing);
  return {
    destroy() {
      preference.removeEventListener('change', settle);
      node.removeEventListener('introstart', opening);
      node.removeEventListener('outrostart', closing);
    },
  };
}
