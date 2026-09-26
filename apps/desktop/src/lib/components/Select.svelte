<script lang="ts" generics="T extends string">
  import { onMount, tick } from 'svelte';
  import Icon from './Icon.svelte';
  import ModelStatus from './ModelStatus.svelte';
  import type { DownloadOptionStatus } from '../modelDownloads';
  import { optionHighlight } from '../actions/selectionHighlight';

  type Option = {
    value: T;
    label: string;
    creator?: string;
    description?: string;
    disabled?: boolean;
    status?: DownloadOptionStatus;
  };
  let {
    id,
    value = $bindable(),
    options,
    disabled = false,
    describedBy,
    invalid = false,
    label,
    identity = false,
    onChange,
  }: {
    id: string;
    value: T;
    options: Option[];
    disabled?: boolean;
    describedBy?: string;
    invalid?: boolean;
    label?: string;
    identity?: boolean;
    onChange?: (value: T) => void;
  } = $props();
  let trigger: HTMLButtonElement;
  let popup = $state<HTMLDivElement>();
  let open = $state(false);
  let active = $state(-1);
  let position = $state({ left: 0, top: 0, bottom: 0, width: 0, height: 240 });
  let upward = $state(false);
  let accessibleLabel = $state('');
  let search = '';
  let searchedAt = 0;
  const selected = $derived(options.find((option) => option.value === value));
  const listId = $derived(`${id}-options`);
  const optionId = (index: number) => `${id}-option-${index}`;
  const available = () =>
    !disabled && !!trigger && !trigger.matches(':disabled') && trigger.getClientRects().length > 0;

  function close() {
    open = false;
    search = '';
  }
  function place() {
    const rect = trigger.getBoundingClientRect();
    const viewport = window.visualViewport;
    const left = viewport?.offsetLeft ?? 0;
    const top = viewport?.offsetTop ?? 0;
    const width = viewport?.width ?? window.innerWidth;
    const height = viewport?.height ?? window.innerHeight;
    const below = top + height - rect.bottom - 12;
    const above = rect.top - top - 12;
    const desired = Math.min(
      options.some((option) => option.status || option.creator) ? 360 : 240,
      options.reduce(
        (height, option) => height + (option.description || option.creator ? 64 : 44),
        10,
      ),
    );
    upward = below < desired && above > below;
    const popupHeight = Math.max(44, Math.min(desired, upward ? above : below));
    const popupWidth = Math.min(rect.width, width - 16);
    position = {
      left: Math.max(left + 8, Math.min(rect.left, left + width - popupWidth - 8)),
      top: rect.bottom + 6,
      bottom: window.innerHeight - rect.top + 6,
      width: popupWidth,
      height: popupHeight,
    };
  }
  async function reveal() {
    await tick();
    if (open) document.getElementById(optionId(active))?.scrollIntoView({ block: 'nearest' });
  }
  function show() {
    if (!available() || !options.some((option) => !option.disabled)) return;
    accessibleLabel = Array.from(trigger.labels ?? [])
      .map((element) => element.textContent?.trim())
      .filter(Boolean)
      .join(' ');
    active = options.findIndex((option) => option.value === value && !option.disabled);
    if (active < 0) active = options.findIndex((option) => !option.disabled);
    place();
    open = true;
    void reveal();
  }
  function choose(index: number) {
    const option = options[index];
    if (!available() || !option || option.disabled) return;
    const changed = value !== option.value;
    value = option.value;
    close();
    trigger.focus({ preventScroll: true });
    if (changed) {
      onChange?.(value);
      trigger.dispatchEvent(new Event('change', { bubbles: true }));
    }
  }
  function move(step: number, edge = false) {
    if (!open) show();
    if (!open) return;
    let index = edge ? (step > 0 ? -1 : options.length) : active;
    for (let count = 0; count < options.length; count++) {
      index = (index + step + options.length) % options.length;
      if (!options[index].disabled) {
        active = index;
        void reveal();
        return;
      }
    }
  }
  function keydown(event: KeyboardEvent) {
    if (!available()) return;
    if (event.key === 'Tab') {
      close();
      return;
    }
    if (event.key === 'Escape') {
      if (open) {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!open) show();
      else move(event.key === 'ArrowDown' ? 1 : -1);
      return;
    }
    if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      move(event.key === 'Home' ? 1 : -1, true);
      return;
    }
    if (
      event.key === 'Enter' ||
      (event.key === ' ' && (!search || Date.now() - searchedAt > 700))
    ) {
      event.preventDefault();
      if (open) choose(active);
      else show();
      return;
    }
    if (event.key.length !== 1 || event.ctrlKey || event.metaKey || event.altKey) return;
    event.preventDefault();
    if (!open) show();
    const now = Date.now();
    search = (now - searchedAt > 700 ? '' : search) + event.key.toLocaleLowerCase();
    searchedAt = now;
    const repeated = [...search].every((character) => character === search[0]);
    const prefix = repeated ? search[0] : search;
    const start = repeated ? active + 1 : active;
    for (let offset = 0; offset < options.length; offset++) {
      const index = (start + offset + options.length) % options.length;
      if (!options[index].disabled && options[index].label.toLocaleLowerCase().startsWith(prefix)) {
        active = index;
        void reveal();
        break;
      }
    }
  }
  function portal(node: HTMLDivElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
  $effect(() => {
    if (disabled || !options.some((option) => !option.disabled)) close();
    else if (open && (!options[active] || options[active].disabled)) {
      active = options.findIndex((option) => !option.disabled);
    }
  });
  onMount(() => {
    const outside = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !trigger.contains(event.target) &&
        !popup?.contains(event.target)
      )
        close();
    };
    const scroll = (event: Event) => {
      if (!(event.target instanceof Node && popup?.contains(event.target))) close();
    };
    const visibility = new MutationObserver(() => {
      if (!available()) close();
    });
    for (let element: HTMLElement | null = trigger; element; element = element.parentElement) {
      visibility.observe(element, { attributes: true, attributeFilter: ['hidden', 'disabled'] });
    }
    document.addEventListener('pointerdown', outside, true);
    window.addEventListener('scroll', scroll, true);
    window.addEventListener('resize', close);
    window.addEventListener('blur', close);
    window.visualViewport?.addEventListener('resize', close);
    return () => {
      visibility.disconnect();
      document.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('scroll', scroll, true);
      window.removeEventListener('resize', close);
      window.removeEventListener('blur', close);
      window.visualViewport?.removeEventListener('resize', close);
    };
  });
</script>

<button
  bind:this={trigger}
  {id}
  type="button"
  class="select-trigger"
  class:is-open={open}
  class:has-status={Boolean(selected?.status)}
  class:quiet-identity={identity}
  {disabled}
  role="combobox"
  aria-expanded={open}
  aria-controls={listId}
  aria-haspopup="listbox"
  aria-label={identity
    ? [label, selected?.creator, selected?.label].filter(Boolean).join(': ')
    : label}
  aria-describedby={describedBy}
  aria-invalid={invalid || undefined}
  aria-activedescendant={open && active >= 0 ? optionId(active) : undefined}
  onclick={() => (open ? close() : show())}
  onkeydown={keydown}
  onblur={close}
>
  <span class="selected-copy"
    >{#if identity && selected?.creator}<span class="creator-name">{selected.creator}</span
      >{/if}<span class="selected-label">{selected?.label ?? value}</span></span
  >
  <svg class="caret" viewBox="0 0 16 16" fill="none" aria-hidden="true"
    ><path
      d="m4 6 4 4 4-4"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    /></svg
  >
</button>
{#if open}
  <div
    bind:this={popup}
    use:portal
    use:optionHighlight={active}
    id={listId}
    class="select-popup"
    class:upward
    role="listbox"
    aria-label={label ?? (accessibleLabel || selected?.label || value)}
    tabindex="-1"
    style:left={`${position.left}px`}
    style:top={upward ? undefined : `${position.top}px`}
    style:bottom={upward ? `${position.bottom}px` : undefined}
    style:width={`${position.width}px`}
    style:max-height={`${position.height}px`}
  >
    {#each options as option, index (option.value)}
      <button
        type="button"
        id={optionId(index)}
        class="select-option"
        class:is-active={active === index}
        class:has-status={Boolean(option.status)}
        role="option"
        aria-label={[option.creator, option.label, option.description, option.status?.label]
          .filter(Boolean)
          .join('. ')}
        aria-selected={option.value === value}
        aria-disabled={option.disabled || undefined}
        disabled={option.disabled}
        tabindex="-1"
        onpointerdown={(event) => event.preventDefault()}
        onpointermove={() => {
          if (!option.disabled) active = index;
        }}
        onclick={() => choose(index)}
      >
        <span class="option-copy"
          ><span class="option-label">{option.label}</span
          >{#if option.creator || option.description}<span class="option-meta"
              >{#if option.creator}<span class="creator-name">{option.creator}</span
                >{/if}{#if option.creator && option.description}<span aria-hidden="true">·</span
                >{/if}{#if option.description}<small>{option.description}</small>{/if}</span
            >{/if}</span
        >
        {#if option.status}<span class="option-status"><ModelStatus status={option.status} /></span
          >{/if}
        <span class="selected-check" aria-hidden="true"
          >{#if option.value === value}<Icon name="check" />{/if}</span
        >
      </button>
    {/each}
  </div>
{/if}

<style>
  .select-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    width: 100%;
    min-width: 0;
    min-height: 44px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--color-control-edge);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-ink);
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 400;
    line-height: 1.5;
    text-align: left;
    cursor: pointer;
    transition:
      background-color 140ms var(--ease),
      border-color 140ms var(--ease);
  }
  .select-trigger:hover:not(:disabled) {
    background: var(--color-panel);
    border-color: var(--color-control-hover);
  }
  .select-trigger:active:not(:disabled),
  .select-trigger.is-open {
    background: var(--color-panel);
    border-color: var(--color-accent);
    transform: none;
  }
  .select-trigger:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 3px;
  }
  .select-trigger[aria-invalid='true'] {
    border-color: var(--color-error);
  }
  .select-trigger:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .selected-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .selected-copy {
    display: grid;
    min-width: 0;
    gap: var(--space-2);
  }
  .creator-name {
    color: var(--color-accent);
    font-size: 11px;
    font-weight: 400;
    line-height: 1.4;
  }
  .quiet-identity {
    padding: 12px 0 18px;
    min-height: 84px;
    border: 0;
    border-bottom: 1px solid var(--color-control-edge);
    border-radius: 0;
    background: var(--color-transparent);
  }
  .quiet-identity:hover:not(:disabled),
  .quiet-identity.is-open,
  .quiet-identity:active:not(:disabled) {
    background: var(--color-transparent);
  }
  .quiet-identity .creator-name {
    font-size: 13px;
  }
  .quiet-identity .selected-label {
    font-size: 22px;
    font-weight: 500;
    line-height: 1.3;
    letter-spacing: -0.5px;
  }
  .quiet-identity .caret {
    width: 20px;
    height: 20px;
  }
  .caret {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: var(--color-muted);
    transition:
      transform var(--duration-state) var(--ease-out),
      color var(--duration-hover) var(--ease-out);
  }
  .is-open .caret {
    transform: rotate(180deg);
    color: var(--color-accent);
  }
  .select-popup {
    position: fixed;
    z-index: 1000;
    box-sizing: border-box;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    padding: 4px;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-panel);
    color: var(--color-ink);
    box-shadow: 0 8px 24px color-mix(in oklch, var(--color-bg) 65%, var(--color-transparent));
    font-family: var(--font-body);
    animation: select-enter 120ms var(--ease);
  }
  .select-popup.upward {
    animation-name: select-enter-up;
  }
  .select-option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    box-sizing: border-box;
    width: 100%;
    min-height: 44px;
    margin: 0;
    padding: 10px 12px;
    border: 0;
    border-radius: 4px;
    background: var(--color-transparent);
    color: var(--color-ink);
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 400;
    line-height: 1.5;
    text-align: left;
    cursor: pointer;
    transition: background-color 100ms var(--ease);
  }
  .select-option.is-active,
  .select-option:hover:not(:disabled),
  .select-option:active:not(:disabled) {
    background: var(--color-transparent);
    transform: none;
  }
  .select-popup {
    --selection-color: var(--color-raised);
  }
  .select-option[aria-selected='true'] {
    color: var(--color-accent);
  }
  .select-option:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .select-option:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: -2px;
  }
  .option-copy {
    display: grid;
    flex: 1;
    min-width: 0;
    gap: 4px;
  }
  .option-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .option-copy small {
    color: var(--color-muted);
    font-size: 11px;
    line-height: 1.4;
    white-space: normal;
  }
  .option-meta {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 2px 6px;
    color: var(--color-muted);
    font-size: 11px;
    line-height: 1.4;
  }
  .option-status {
    display: flex;
    justify-content: flex-end;
    flex: 0 0 auto;
    max-width: 132px;
  }
  .option-status :global(.status-glyph) {
    width: 18px;
    height: 18px;
  }
  .selected-check {
    display: flex;
    flex-shrink: 0;
    width: 16px;
    height: 16px;
  }
  .selected-check :global(svg) {
    width: 16px;
    height: 16px;
  }
  .selected-check :global(path) {
    stroke-dasharray: 1;
    animation: selected-check-draw 180ms var(--ease) both;
  }
  @keyframes selected-check-draw {
    from {
      stroke-dashoffset: 1;
    }
    to {
      stroke-dashoffset: 0;
    }
  }
  @keyframes select-enter {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @keyframes select-enter-up {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (max-width: 560px) {
    .has-status .option-label,
    .has-status .selected-label {
      white-space: normal;
      overflow-wrap: anywhere;
    }
    .option-status {
      max-width: 80px;
    }
    .select-option {
      gap: var(--space-2);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .select-trigger,
    .caret,
    .select-option {
      transition: none;
    }
    .select-popup,
    .selected-check :global(path) {
      animation: none;
    }
  }
</style>
