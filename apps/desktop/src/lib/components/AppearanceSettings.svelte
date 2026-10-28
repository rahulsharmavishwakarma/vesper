<script lang="ts">
  import { onMount } from 'svelte';
  import { APPEARANCE_KEY, readAppearance, saveAppearance, type Appearance } from '../appearance';
  import Icon from './Icon.svelte';
  let selected = $state<Appearance>(readAppearance());
  let error = $state('');
  let reducedMotion = $state(false);
  const choices: { value: Appearance; label: string }[] = [
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
    { value: 'system', label: 'System' },
  ];
  function choose(value: Appearance) {
    try {
      saveAppearance(value);
      selected = value;
      error = '';
    } catch {
      error = 'Could not save appearance. Allow local storage and try again.';
    }
  }
  onMount(() => {
    const motion = matchMedia('(prefers-reduced-motion: reduce)');
    const updateMotion = () => {
      reducedMotion = motion.matches;
    };
    const refresh = () => {
      selected = readAppearance();
    };
    const storage = (event: StorageEvent) => {
      if (event.key === APPEARANCE_KEY || event.key === null) refresh();
    };
    updateMotion();
    motion.addEventListener('change', updateMotion);
    window.addEventListener('storage', storage);
    window.addEventListener('vesper-appearance-change', refresh);
    return () => {
      motion.removeEventListener('change', updateMotion);
      window.removeEventListener('storage', storage);
      window.removeEventListener('vesper-appearance-change', refresh);
    };
  });
</script>

<div class="appearance-options" role="group" aria-label="Color theme">
  {#each choices as choice (choice.value)}
    <button
      type="button"
      aria-pressed={selected === choice.value}
      onclick={() => choose(choice.value)}
    >
      <span
        class="theme-mini"
        class:mini-light={choice.value === 'light'}
        class:mini-dark={choice.value === 'dark'}
        class:mini-system={choice.value === 'system'}
        aria-hidden="true"><i></i><b></b><b></b></span
      >
      <span>{choice.label}</span><Icon name="check" />
    </button>
  {/each}
</div>
{#if error}<p class="error" role="alert">{error}</p>{/if}
<div class="appearance-motion">
  <span>Reduce motion</span><span class="note"
    >{reducedMotion ? 'On · follows system' : 'Off · follows system'}</span
  >
</div>
