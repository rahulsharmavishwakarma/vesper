/** A short release settle for mouse, touch and keyboard; CSS handles the initial press. */
export function installPressFeedback(): () => void {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  const running = new Set<Animation>();
  const buttons = new WeakMap<HTMLButtonElement, Animation>();
  const cancel = () => {
    for (const animation of running) animation.cancel();
    running.clear();
  };
  const activate = (event: MouseEvent) => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      cancel();
      return;
    }
    if (!(event.target instanceof Element)) return;
    const button = event.target.closest<HTMLButtonElement>('button');
    if (
      !button ||
      button.matches(':disabled') ||
      button.getAttribute('aria-disabled') === 'true' ||
      typeof button.animate !== 'function'
    )
      return;
    // Menus use their own surface/caret feedback. Scaling a trigger before its
    // click handler measures it would shift the anchored popup's geometry.
    if (
      button.matches(
        '[role="combobox"], [role="option"], .activity-day, .session-button, .capture-brand, .brand',
      ) ||
      button.closest('.sliding-selection, .sidebar nav')
    )
      return;
    buttons.get(button)?.cancel();
    const animation = button.animate(
      [
        { transform: 'translateY(1px) scale(.985)', offset: 0 },
        { transform: 'translateY(0) scale(1)', offset: 1 },
      ],
      {
        duration: 200,
        easing:
          getComputedStyle(button).getPropertyValue('--ease-out').trim() ||
          'cubic-bezier(.2, .8, .2, 1)',
      },
    );
    buttons.set(button, animation);
    running.add(animation);
    const done = () => running.delete(animation);
    animation.addEventListener('finish', done, { once: true });
    animation.addEventListener('cancel', done, { once: true });
  };
  const changed = () => {
    if (preference.matches) cancel();
  };
  document.addEventListener('click', activate, true);
  preference.addEventListener('change', changed);
  window.addEventListener('blur', cancel);
  return () => {
    cancel();
    document.removeEventListener('click', activate, true);
    preference.removeEventListener('change', changed);
    window.removeEventListener('blur', cancel);
  };
}
