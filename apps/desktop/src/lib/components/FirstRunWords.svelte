<script lang="ts">
  import { onMount, tick } from 'svelte';
  import artwork from '../../assets/ember-bird.svg?raw';
  import Icon from './Icon.svelte';
  import {
    getPersonalization,
    savePersonalization,
    type PersonalizationProfile,
  } from '../api/personalization';
  import type { AppConfig } from '../config';
  import { SAMPLE_HEARD, sampleWritten, vocabularyWord } from '../firstRun';

  let {
    settings,
    preview = false,
    onSaveSettings,
    onWords,
  }: {
    settings: AppConfig;
    preview?: boolean;
    onSaveSettings: (next: AppConfig) => Promise<AppConfig>;
    onWords: (words: string[]) => void;
  } = $props();

  const STYLES = [
    {
      value: 'minimal',
      title: 'As I said it',
      detail: 'Adds punctuation and capitals. Keeps every word you say.',
    },
    {
      value: 'balanced',
      title: 'Polished',
      detail: 'Also drops fillers and fixes what was misheard.',
    },
  ] as const;

  let profile: PersonalizationProfile | null = null;
  let words = $state<string[]>([]);
  let draft = $state('');
  let message = $state<string | null>(null);
  let swapping = $state(false);
  let birdMood = $state<'' | 'tilt' | 'call'>('');
  let landing = $state<string | null>(null);
  let flight = $state<{ word: string } | null>(null);
  let flyer: HTMLSpanElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();
  let root: HTMLElement | undefined = $state();
  const tidy = $derived(settings.cleanupEnabled);
  const style = $derived(settings.cleanupPreset);
  const tokens = $derived(sampleWritten(style, words));
  // Each word with what follows it (punctuation, then a space between words).
  const parts = $derived(
    tokens.map((token, index) => {
      const word = token.taught ? token.text.replace(/[?,]$/, '') : token.text;
      const space = index < tokens.length - 1 ? ' ' : '';
      return { word, taught: token.taught, tail: token.text.slice(word.length) + space };
    }),
  );
  const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
  /** A theme colour as rgba, for animation keyframes. */
  function tokenColour(name: string, alpha: number): string {
    const hex = getComputedStyle(document.documentElement)
      .getPropertyValue(name)
      .trim()
      .replace('#', '');
    const n = Number.parseInt(hex.length === 3 ? hex.replace(/./g, '$&$&') : hex, 16);
    return Number.isNaN(n)
      ? `rgba(0,0,0,${alpha})`
      : `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha.toFixed(3)})`;
  }

  $effect(() => onWords([...words]));

  function react(tilt: boolean) {
    birdMood = '';
    void tick().then(() => (birdMood = tilt ? 'tilt' : 'call'));
    setTimeout(() => (birdMood = ''), 1000);
  }

  async function chooseStyle(value: 'minimal' | 'balanced') {
    if (!tidy || value === style) return;
    if (!reduced()) {
      swapping = true;
      setTimeout(() => (swapping = false), 160);
    }
    react(true);
    try {
      await onSaveSettings({ ...settings, cleanupPreset: value });
    } catch (cause) {
      message = `Could not save the style. ${String(cause)}`;
    }
  }

  async function persist(next: string[]) {
    if (preview || !profile) return;
    const manual = new Set(next.map((word) => word.toLowerCase()));
    const kept = profile.vocabulary.filter(
      (entry) => entry.automaticSource !== null || manual.has(entry.phrase.toLowerCase()),
    );
    const known = new Set(kept.map((entry) => entry.phrase.toLowerCase()));
    const added = next
      .filter((word) => !known.has(word.toLowerCase()))
      .map((phrase) => ({
        id: crypto.randomUUID(),
        phrase,
        language: 'auto' as const,
        context: '',
        enabled: true,
        confirmed: true,
        aliases: [],
        project: profile!.activeProject,
        pronunciation: false,
        sources: [],
        learnedAt: '',
        automaticSource: null,
        usageCount: 0,
      }));
    profile = await savePersonalization({ ...profile, vocabulary: [...added, ...kept] });
  }

  async function add(raw: string, from: HTMLElement | undefined = input) {
    const result = vocabularyWord(raw, words);
    if ('error' in result) {
      message = result.error;
      input?.focus();
      return;
    }
    message = null;
    const start = from?.getBoundingClientRect();
    words = [...words, result.word];
    draft = '';
    try {
      await persist(words);
    } catch (cause) {
      words = words.filter((word) => word !== result.word);
      message = `Could not save “${result.word}”. ${String(cause)}`;
      return;
    }
    input?.focus();
    if (!tidy) return react(false);
    const taught = tokens.some(
      (token) => token.taught && token.text.replace(/[?,]$/, '') === result.word,
    );
    if (!taught || reduced() || !start || !root) return react(false);
    // The word travels as a chip from where it was typed into its place in the
    // sentence (across, then up into its slot), and the bird chirps on landing.
    landing = result.word;
    flight = { word: result.word };
    await tick();
    const target = root.querySelector<HTMLElement>(`mark[data-word="${CSS.escape(result.word)}"]`);
    const base = root.getBoundingClientRect();
    if (!target || !flyer) {
      flight = null;
      landing = null;
      return react(false);
    }
    const to = target.getBoundingClientRect();
    const a = { x: start.left - base.left + 12, y: start.top - base.top + 4 };
    const b = { x: to.left - base.left, y: to.top - base.top };
    const frames: Keyframe[] = [];
    for (let i = 0; i <= 20; i++) {
      const t = i / 20;
      const u = 1 - t;
      const chip = t < 0.72 ? 1 : Math.max(0, (1 - t) / 0.28);
      frames.push({
        transform: `translate(${(u * u * a.x + 2 * u * t * b.x + t * t * b.x).toFixed(1)}px, ${(u * u * a.y + 2 * u * t * a.y + t * t * b.y).toFixed(1)}px) scale(${(0.7 + 0.3 * t).toFixed(3)})`,
        opacity: t < 0.1 ? t * 10 : 1,
        backgroundColor: tokenColour('--color-raised', chip),
        borderColor: tokenColour('--color-line', chip),
      });
    }
    flyer.animate(frames, {
      duration: 700,
      easing: 'cubic-bezier(.6,0,.25,1)',
      fill: 'forwards',
    }).onfinish = () => {
      flight = null;
      landing = null;
      react(false);
    };
  }

  async function remove(word: string) {
    const previous = words;
    words = words.filter((entry) => entry !== word);
    try {
      await persist(words);
    } catch (cause) {
      words = previous;
      message = `Could not remove “${word}”. ${String(cause)}`;
    }
    input?.focus();
  }

  onMount(async () => {
    if (preview) return;
    try {
      profile = await getPersonalization();
      words = profile.vocabulary
        .filter((entry) => entry.automaticSource === null)
        .map((entry) => entry.phrase)
        .slice(0, 24);
    } catch {
      profile = null;
    }
  });
</script>

<div class="card" bind:this={root}>
  <div class="preview" aria-live="polite">
    <button
      type="button"
      class="bird"
      class:tilt={birdMood === 'tilt'}
      class:calling={birdMood !== ''}
      aria-label="Vesper"
      onclick={() => react(true)}
    >
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html artwork}
    </button>
    <div>
      <p class="heard">You say: <i>“{SAMPLE_HEARD}”</i></p>
      <p class="written" class:swap={swapping}>
        {#if !tidy}{SAMPLE_HEARD}{:else}
          {#each parts as part, index (index)}
            {#if part.taught}<mark data-word={part.word} class:wait={landing === part.word}
                >{part.word}</mark
              >{part.tail}{:else}{part.word + part.tail}{/if}
          {/each}
        {/if}
      </p>
    </div>
  </div>
  <section class="sec">
    <p class="lab"><Icon name="spark" />How should your words read?</p>
    <div class="styles" role="radiogroup" aria-label="Writing style">
      {#each STYLES as option (option.value)}
        <button
          type="button"
          class="style"
          role="radio"
          aria-checked={style === option.value}
          aria-disabled={!tidy}
          onclick={() => void chooseStyle(option.value)}
          ><b>{option.title}</b><span>{option.detail}</span></button
        >
      {/each}
    </div>
    {#if !tidy}<p class="hint">
        Tidy my text is off, so Vesper types exactly what it hears. Turn it on in Models to choose a
        style.
      </p>{/if}
  </section>
  <section class="sec">
    <p class="lab"><Icon name="book" />Words Vesper should know</p>
    <form
      class="add"
      onsubmit={(event) => {
        event.preventDefault();
        void add(draft);
      }}
    >
      <input
        bind:this={input}
        bind:value={draft}
        maxlength="40"
        autocomplete="off"
        placeholder="A name, product or place"
        aria-label="Word to add"
        aria-describedby="setup-words-message"
      />
      <button type="submit">Add</button>
    </form>
    <div class="chips">
      {#each words as word (word)}
        <span class="chip"
          >{word}<button
            type="button"
            aria-label={`Remove ${word}`}
            onclick={() => void remove(word)}><Icon name="close" /></button
          ></span
        >
      {/each}
      {#if !words.some((word) => word.toLowerCase() === 'arclight')}
        <button
          type="button"
          class="chip try"
          onclick={(event) => void add('Arclight', event.currentTarget)}>Try “Arclight”</button
        >
      {/if}
    </div>
    <p
      id="setup-words-message"
      class="hint"
      class:error={Boolean(message)}
      role={message ? 'alert' : undefined}
    >
      {message ?? 'Optional. Add as many as you like, now or later.'}
    </p>
    <p class="learn">
      <Icon name="history" />Vesper also learns from your corrections, on this Mac only. Review or
      forget anything in Settings.
    </p>
  </section>
  {#if flight}<span class="flyer" bind:this={flyer} aria-hidden="true">{flight.word}</span>{/if}
</div>

<style>
  .card {
    position: relative;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
  }
  .preview {
    display: grid;
    grid-template-columns: 64px 1fr;
    gap: 18px;
    align-items: center;
    padding: 22px 26px;
  }
  .bird {
    all: unset;
    cursor: pointer;
    width: 64px;
    height: 48px;
    color: var(--color-accent);
    transform-origin: 40% 85%;
  }
  .bird:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 6px;
    border-radius: 10px;
  }
  .bird :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
    overflow: visible;
  }
  .bird :global(.bd),
  .bird :global(.dsh),
  .bird :global(.lid) {
    fill: currentColor;
  }
  .bird :global(.dsh) {
    transform-box: fill-box;
    transform-origin: left center;
  }
  .bird :global(.lid) {
    transform-box: fill-box;
    transform-origin: center;
    transform: scaleY(0);
    animation: blink 4.2s ease-in-out infinite;
  }
  .bird.tilt {
    animation: tilt 0.7s var(--ease-spring);
  }
  .bird.calling :global(.dsh) {
    animation: call 0.9s ease-out both;
  }
  .heard {
    margin: 0 0 6px;
    font-size: 13px;
    color: var(--color-muted);
  }
  .heard i {
    font-style: normal;
    color: var(--color-control-hover);
  }
  .written {
    margin: 0;
    min-height: 1.35em;
    font: 600 22px/1.35 var(--font-display);
    letter-spacing: -0.015em;
    transition:
      opacity 0.16s var(--ease),
      filter 0.16s var(--ease);
  }
  .written.swap {
    opacity: 0;
    filter: blur(4px);
  }
  mark {
    color: inherit;
    padding: 0 1px;
    background: linear-gradient(var(--color-accent), var(--color-accent)) left bottom / 100% 2px
      no-repeat;
    animation: underline 0.5s var(--ease-spring);
  }
  mark.wait {
    opacity: 0;
    background-size: 0 2px;
    animation: none;
  }
  .sec {
    padding: 22px 26px;
    border-top: 1px solid var(--color-line);
  }
  .lab {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin: 0 0 10px;
  }
  .lab :global(svg),
  .learn :global(svg) {
    width: 16px;
    height: 16px;
    color: var(--color-accent);
    flex: none;
  }
  .styles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .style {
    all: unset;
    cursor: pointer;
    box-sizing: border-box;
    padding: 16px 18px;
    border: 1px solid var(--color-line);
    border-radius: 16px;
    background: var(--color-bg);
  }
  .style b {
    display: block;
    font-weight: 600;
    font-size: 16px;
    margin-bottom: 3px;
  }
  .style span {
    display: block;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--color-muted);
  }
  .style[aria-checked='true'] {
    border-color: var(--color-accent);
    background: var(--color-tint);
  }
  .style[aria-checked='true'] b {
    color: var(--color-accent-light);
  }
  .style[aria-disabled='true'] {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .style:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .add {
    display: flex;
    gap: 10px;
  }
  .add input {
    flex: 1;
    min-width: 0;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 12px;
    min-height: 34px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 5px 6px 5px 12px;
    border-radius: 99px;
    border: 1px solid var(--color-line);
    background: var(--color-bg);
    font-size: 14px;
    animation: chip-in 0.45s var(--ease-spring);
  }
  .chip button {
    all: unset;
    cursor: pointer;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--color-muted);
  }
  .chip button:focus-visible {
    outline: 2px solid var(--color-focus);
  }
  .chip button :global(svg) {
    width: 12px;
    height: 12px;
  }
  .chip.try {
    padding: 5px 12px;
    border-style: dashed;
    color: var(--color-muted);
    cursor: pointer;
    animation: none;
  }
  .hint {
    margin: 10px 0 0;
    font-size: 13px;
    color: var(--color-muted);
  }
  .hint.error {
    color: var(--color-error);
  }
  .learn {
    display: flex;
    gap: 10px;
    align-items: center;
    margin: 14px 0 0;
    font-size: 13.5px;
    color: var(--color-muted);
  }
  .flyer {
    position: absolute;
    left: 0;
    top: 0;
    z-index: 5;
    padding: 0 10px;
    margin: 0 -10px;
    border: 1px solid transparent;
    border-radius: 99px;
    font: 600 22px/1.35 var(--font-display);
    letter-spacing: -0.015em;
    color: var(--color-accent);
    white-space: nowrap;
    pointer-events: none;
    transform-origin: 0 50%;
  }
  @keyframes tilt {
    35% {
      transform: rotate(-10deg) translateY(-2px);
    }
  }
  @keyframes call {
    0% {
      opacity: 0.1;
      transform: scale(0.4);
    }
    45% {
      opacity: 1;
      transform: scale(1.25);
    }
  }
  @keyframes blink {
    0%,
    90%,
    100% {
      transform: scaleY(0);
    }
    94%,
    96% {
      transform: scaleY(1);
    }
  }
  @keyframes underline {
    from {
      background-size: 0 2px;
    }
  }
  @keyframes chip-in {
    from {
      transform: scale(0.6);
      opacity: 0;
    }
  }
  @container first-run (max-width: 760px) {
    .styles {
      grid-template-columns: 1fr;
    }
    .preview,
    .sec {
      padding: 18px;
    }
    .preview {
      grid-template-columns: 44px 1fr;
    }
    .bird {
      width: 44px;
      height: 33px;
    }
    .written {
      font-size: 18px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bird.tilt,
    .bird.calling :global(.dsh),
    .bird :global(.lid),
    mark,
    .chip {
      animation: none;
    }
  }
</style>
