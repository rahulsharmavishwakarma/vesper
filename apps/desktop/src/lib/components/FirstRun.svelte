<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import artwork from '../../assets/ember-bird.svg?raw';
  import BrandLogo from './BrandLogo.svelte';
  import Icon from './Icon.svelte';
  import FirstRunVoice from './FirstRunVoice.svelte';
  import FirstRunModels from './FirstRunModels.svelte';
  import FirstRunAccess from './FirstRunAccess.svelte';
  import FirstRunShortcut from './FirstRunShortcut.svelte';
  import FirstRunWords from './FirstRunWords.svelte';
  import FirstRunTry from './FirstRunTry.svelte';
  import { getModelInstallation, installModel, type InstallProgress } from '../api/runtime';
  import { downloadTarget, type DownloadTarget } from '../modelDownloads';
  import { completeOnboarding, saveOnboardingStep } from '../api/onboarding';
  import {
    FIRST_RUN_STEPS,
    READY_STEP,
    SETUP_LANGUAGES,
    resumeStep,
    type SetupLanguage,
  } from '../firstRun';
  import {
    MODEL_ROLE_LABELS,
    recordingShortcuts,
    transcriptionLanguageOptions,
    type AppConfig,
  } from '../config';
  import { shortcutLabel } from '../recording-shortcuts';
  import { recommendedSpeechModel } from '../recommendations';

  let {
    settings,
    initialStep = 0,
    persist = true,
    progress = null,
    onSaveSettings,
    onDone,
  }: {
    settings: AppConfig | null;
    initialStep?: number;
    /** False in previews: nothing is saved. */
    persist?: boolean;
    /** Progress of the model download that is running, if any. */
    progress?: InstallProgress | null;
    /** Save settings through the app, so the rest of the app sees them. */
    onSaveSettings: (next: AppConfig) => Promise<AppConfig>;
    onDone: () => void;
  } = $props();

  // The copy each step opens with (issue #270). Steps 1–6 are filled in by
  // later phases; the flow stays disabled for users until then.
  const STEP_COPY: Record<number, { title: string; lede: string }> = {
    1: {
      title: 'What will you speak?',
      lede: 'Language decides which models we suggest. Check your microphone while you’re here.',
    },
    2: {
      title: 'Models for this Mac.',
      lede: 'One tap downloads a setup chosen for your Mac. Keep going while it installs.',
    },
    3: {
      title: 'Let Vesper type for you.',
      lede: 'Accessibility lets Vesper put your words into whatever app you’re using.',
    },
    4: { title: 'Your shortcut.', lede: 'Dictate from anywhere. These work in every app.' },
    5: {
      title: 'Make it yours.',
      lede: 'Choose how your words read, and teach Vesper the names you use.',
    },
    6: {
      title: 'Say something.',
      lede: 'Hold your shortcut and say the sentence from the last step. It appears here, written your way.',
    },
  };

  // The saved step is read once, when setup opens.
  // svelte-ignore state_referenced_locally
  const startStep = resumeStep(initialStep);
  let step = $state(startStep);
  let phase = $state<'welcome' | 'leaving' | 'steps'>(startStep === 0 ? 'welcome' : 'steps');
  let flight = $state<{ width: number; height: number } | null>(null);
  let flyer: HTMLDivElement | undefined = $state();
  let birdIn = $state(false);
  let textIn = $state(false);
  let enteredFromWelcome = $state(false);
  let finishing = $state(false);
  let error = $state<string | null>(null);

  let root: HTMLElement | undefined = $state();
  let heroBird: HTMLButtonElement | undefined = $state();
  let reedPath: SVGPathElement | undefined = $state();
  let chirping = $state(false);
  let frame = 0;
  const timers: ReturnType<typeof setTimeout>[] = [];
  const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

  const shortcuts = $derived(settings ? recordingShortcuts(settings) : null);

  // Voice and Models (phase 2).
  const isSetupLanguage = (value: unknown): value is SetupLanguage =>
    SETUP_LANGUAGES.some((option) => option.value === value);
  // svelte-ignore state_referenced_locally
  let language = $state<SetupLanguage>(
    isSetupLanguage(settings?.language) ? settings.language : 'en',
  );
  let voiceReady = $state(false);
  let accessReady = $state(false);
  let words = $state<string[]>([]);
  let tried = $state(false);
  let models:
    | {
        plan: () => { settings: AppConfig; targets: DownloadTarget[]; bytes: number };
        blocker: () => string | null;
      }
    | undefined = $state();
  let install = $state<{
    status: 'idle' | 'running' | 'done' | 'error';
    total: number;
    done: number;
    error: string | null;
  }>({ status: 'idle', total: 0, done: 0, error: null });
  let lastPlan: { settings: AppConfig; targets: DownloadTarget[]; bytes: number } | null = null;

  async function chooseLanguage(next: SetupLanguage) {
    language = next;
    if (!settings) return;
    // Keep the saved settings valid: a language the speech model cannot hear
    // switches to the recommended model for it.
    const supported = transcriptionLanguageOptions(settings.models, settings.computeBackend).some(
      (option) => option.value === next,
    );
    try {
      await onSaveSettings({
        ...settings,
        language: next,
        models: supported
          ? settings.models
          : {
              ...settings.models,
              asr: recommendedSpeechModel(
                next,
                null,
                settings.computeBackend,
              ) as AppConfig['models']['asr'],
              customAsr: null,
            },
      });
    } catch (cause) {
      error = `Could not save the language. ${String(cause)}`;
    }
  }

  /** Save the chosen setup, then download it in the background while the
   * person carries on. Each model downloads in turn. */
  async function startInstall(plan = models?.plan() ?? lastPlan) {
    if (!plan || install.status === 'running') return;
    lastPlan = plan;
    error = null;
    try {
      await onSaveSettings(plan.settings);
    } catch (cause) {
      error = `Could not save your models. ${String(cause)}`;
      return;
    }
    const pending = plan.bytes > 0 ? plan.targets : [];
    install = {
      status: pending.length ? 'running' : 'done',
      total: pending.length,
      done: 0,
      error: null,
    };
    // Downloads continue in the background; setup moves on straight away.
    if (pending.length) void download(pending);
  }

  async function download(pending: DownloadTarget[]) {
    for (const target of pending) {
      try {
        await installModel(target.modelId, target.quantization);
        install.done += 1;
      } catch (cause) {
        install = { ...install, status: 'error', error: String(cause) };
        return;
      }
    }
    install.status = 'done';
  }

  // After a restart the download state is gone: check what is installed and
  // pick the download up again if anything is missing.
  async function ensureInstalled() {
    if (install.status !== 'idle' || !settings) return;
    try {
      if ((await getModelInstallation()).ready) {
        install = { status: 'done', total: 0, done: 0, error: null };
        return;
      }
    } catch {
      // Fall through to a download, which skips files that are already present.
    }
    const targets = [downloadTarget(settings, 'asr')];
    if (settings.cleanupEnabled) targets.push(downloadTarget(settings, 'rewriter'));
    void startInstall({ settings, targets, bytes: 1 });
  }
  $effect(() => {
    if (step === 6) void ensureInstalled();
  });

  const percent = $derived.by(() => {
    if (install.status === 'done') return 100;
    if (!install.total) return 0;
    const file =
      progress && progress.fileCount
        ? (progress.fileIndex -
            1 +
            (progress.totalBytes ? progress.completedBytes / progress.totalBytes : 0)) /
          progress.fileCount
        : 0;
    return Math.min(99, Math.floor(((install.done + file) / install.total) * 100));
  });

  /** Why Continue is not available on this step, if anything. */
  const waiting = $derived.by(() => {
    if (step === 1 && !voiceReady) return 'Allow the microphone and say a few words.';
    if (step === 3 && !accessReady) return 'Turn on Accessibility, or keep your text in Vesper.';
    if (step === 6 && !tried)
      return install.status === 'done'
        ? 'Hold Record and say something.'
        : 'Waiting for the models to finish.';
    if (step === 2 && install.status === 'idle') return models?.blocker() ?? null;
    return null;
  });

  // Welcome background: a fine dot field that answers the bird. Each call
  // sends a soft ripple out from its beak; dots near the pointer brighten and
  // drift aside. Only the bird's body stays clear, so dots never show through
  // its eye or wing. Reduced motion draws one still frame.
  let dotCanvas: HTMLCanvasElement | undefined = $state();
  let ripple: () => void = () => {};
  function startDots(): () => void {
    const canvas = dotCanvas;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return () => {};
    const [r, g, b] = (getComputedStyle(canvas).color.match(/\d+/g) ?? ['255', '107', '61']).map(
      Number,
    );
    const still = reducedMotion();
    let width = 0;
    let height = 0;
    let bird = { x: 0, y: 0, w: 0, h: 0 };
    const measure = () => {
      const ratio = Math.min(2, devicePixelRatio || 1);
      width = canvas.clientWidth;
      height = canvas.clientHeight;
      canvas.width = Math.round(width * ratio);
      canvas.height = Math.round(height * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      const box = canvas.getBoundingClientRect();
      const mark = heroBird?.getBoundingClientRect();
      if (mark)
        bird = { x: mark.left - box.left, y: mark.top - box.top, w: mark.width, h: mark.height };
    };
    const observer = new ResizeObserver(measure);
    observer.observe(canvas);
    measure();
    const ripples: { start: number; x: number; y: number }[] = [];
    ripple = () => {
      measure();
      ripples.push({
        start: performance.now(),
        x: bird.x + bird.w * 0.82,
        y: bird.y + bird.h * 0.4,
      });
    };
    let pointer = { x: -1e4, y: -1e4 };
    let eased = { x: -1e4, y: -1e4 };
    const host = canvas.parentElement!;
    const move = (event: PointerEvent) => {
      const box = canvas.getBoundingClientRect();
      pointer = { x: event.clientX - box.left, y: event.clientY - box.top };
      if (eased.x < -1e3) eased = { ...pointer };
    };
    const leave = () => (pointer = { x: -1e4, y: -1e4 });
    host.addEventListener('pointermove', move);
    host.addEventListener('pointerleave', leave);
    const GAP = 24;
    let raf = 0;
    let running = true;
    const draw = (now: number) => {
      // The pointer highlight eases after the cursor instead of snapping.
      if (pointer.x > -1e3) {
        eased = {
          x: eased.x + (pointer.x - eased.x) * 0.12,
          y: eased.y + (pointer.y - eased.y) * 0.12,
        };
      } else eased = { x: -1e4, y: -1e4 };
      while (ripples.length && now - ripples[0].start > 3200) ripples.shift();
      context.clearRect(0, 0, width, height);
      context.fillStyle = `rgb(${r},${g},${b})`;
      const cx = bird.x + bird.w * 0.42;
      const cy = bird.y + bird.h * 0.55;
      const keep = bird.h * 0.42;
      for (let y = GAP / 2; y < height; y += GAP)
        for (let x = GAP / 2; x < width; x += GAP) {
          let wave = 0;
          for (const item of ripples) {
            const seconds = (now - item.start) / 1000;
            const distance = Math.hypot(x - item.x, y - item.y);
            wave +=
              Math.exp(-(((distance - seconds * 230) / 46) ** 2)) *
              0.42 *
              Math.max(0, 1 - seconds / 3) *
              Math.max(0.25, 1 - distance / 900);
          }
          const dx = x - eased.x;
          const dy = y - eased.y;
          const reach = Math.hypot(dx, dy);
          const near = reach < 150 ? (1 - reach / 150) ** 2 : 0;
          const shift = near && reach > 0 ? (near * 5) / reach : 0;
          const edge = Math.max(0, Math.min(1, (Math.hypot(x - cx, y - cy) - keep) / 14));
          const alpha = (0.1 + wave + near * 0.5) * edge * edge * (3 - 2 * edge);
          if (alpha < 0.02) continue;
          context.globalAlpha = Math.min(0.75, alpha);
          context.beginPath();
          context.arc(x + dx * shift, y + dy * shift, 1 + near * 0.5 + wave * 0.6, 0, Math.PI * 2);
          context.fill();
        }
      if (running && !still) raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    // The bird calls every few seconds on Welcome, and the dots answer it.
    const talk = still ? undefined : setInterval(() => birdIn && chirp(), 5200);
    return () => {
      running = false;
      cancelAnimationFrame(raf);
      clearInterval(talk);
      observer.disconnect();
      host.removeEventListener('pointermove', move);
      host.removeEventListener('pointerleave', leave);
    };
  }

  function chirp() {
    ripple();
    chirping = false;
    void tick().then(() => (chirping = true));
    timers.push(setTimeout(() => (chirping = false), 1200));
  }

  // Welcome: a thin voice line speaks, gathers into the centre and becomes the
  // bird; then the words come into focus and the bird chirps once.
  function playWelcome() {
    if (reducedMotion()) {
      birdIn = true;
      textIn = true;
      return;
    }
    const strokes = 41;
    const SPEAK = 1150;
    const GATHER = 620;
    const start = performance.now();
    const draw = (now: number) => {
      const t = now - start;
      const g = Math.max(0, Math.min(1, (t - SPEAK) / GATHER));
      const gathered = g * g * (3 - 2 * g);
      const intro = Math.min(1, t / 260);
      let d = '';
      for (let i = 0; i < strokes; i++) {
        const x = i / (strokes - 1);
        const envelope = Math.sin(Math.PI * x) ** 1.4;
        const voice =
          0.25 +
          0.75 * Math.abs(Math.sin(x * 7.5 - t / 170)) * (0.6 + 0.4 * Math.sin(t / 240 + x * 3));
        const height = 1.6 + intro * (1 - gathered) * envelope * voice * 74;
        const px = 160 + (x * 316 + 2 - 160) * (1 - gathered * 0.92);
        d += `M${px.toFixed(1)} ${(50 - height / 2).toFixed(1)}V${(50 + height / 2).toFixed(1)}`;
      }
      reedPath?.setAttribute('d', g < 1 ? d : '');
      reedPath?.style.setProperty('opacity', (1 - gathered).toFixed(3));
      if (!birdIn && t > SPEAK + GATHER * 0.55) {
        birdIn = true;
        timers.push(setTimeout(chirp, 620));
      }
      if (g < 1) frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    timers.push(setTimeout(() => (textIn = true), SPEAK + GATHER + 60));
  }

  function save(next: number) {
    if (!persist) return;
    void saveOnboardingStep(next).catch(() => {
      // Progress is a convenience; setup still works if this save fails.
    });
  }

  function go(next: number) {
    if (next === 0) return void returnToWelcome();
    step = next;
    save(next);
    error = null;
    void tick().then(() => root?.querySelector<HTMLElement>('h1')?.focus({ preventScroll: true }));
  }

  // Back from Voice, or Welcome in the rail: the Welcome screen itself comes
  // back, already composed, rather than an empty step.
  let returning = $state(false);
  async function returnToWelcome() {
    step = 0;
    save(0);
    error = null;
    enteredFromWelcome = false;
    birdIn = true;
    textIn = true;
    returning = true;
    phase = 'welcome';
    await tick();
    stopDots = startDots();
    root?.querySelector<HTMLElement>('.get-started')?.focus({ preventScroll: true });
  }

  // Get started: step 2 is placed underneath at once, Welcome crossfades away
  // above it, and the bird is the one element that travels, into its logo spot.
  async function leaveWelcome() {
    if (phase !== 'welcome') return;
    const from = heroBird?.getBoundingClientRect();
    step = 1;
    save(1);
    if (reducedMotion() || !from) {
      phase = 'steps';
      return;
    }
    enteredFromWelcome = true;
    phase = 'leaving';
    await tick();
    const target = root?.querySelector<SVGElement>('.fr-rail .brand-logo svg');
    const rail = root?.querySelector<HTMLElement>('.fr-rail');
    if (target && rail && root) {
      // Measure the logo where it will rest, not mid-slide.
      rail.style.animation = 'none';
      const to = target.getBoundingClientRect();
      void rail.offsetWidth;
      rail.style.animation = '';
      const base = root.getBoundingClientRect();
      flight = { width: from.width, height: from.height };
      await tick();
      target.style.opacity = '0';
      const a = { x: from.left - base.left, y: from.top - base.top };
      const b = { x: to.left - base.left, y: to.top - base.top };
      const scale = to.width / from.width;
      const cx = (a.x + b.x) / 2 + 30;
      const cy = (a.y + b.y) / 2 - 70;
      const frames: Keyframe[] = [];
      for (let i = 0; i <= 24; i++) {
        const t = i / 24;
        const u = 1 - t;
        const x = u * u * a.x + 2 * u * t * cx + t * t * b.x;
        const y = u * u * a.y + 2 * u * t * cy + t * t * b.y;
        frames.push({
          transform: `translate(${x.toFixed(1)}px, ${y.toFixed(1)}px) scale(${(1 + (scale - 1) * t).toFixed(4)}) rotate(${(-7 * Math.sin(Math.PI * t)).toFixed(2)}deg)`,
        });
      }
      const bird = flyer;
      if (bird)
        bird.animate(frames, {
          duration: 1050,
          easing: 'cubic-bezier(.6,0,.25,1)',
          fill: 'forwards',
        }).onfinish = () => {
          target.style.opacity = '';
          bird.animate([{ opacity: 1 }, { opacity: 0 }], {
            duration: 120,
            fill: 'forwards',
          }).onfinish = () => (flight = null);
        };
    }
    timers.push(
      setTimeout(() => (phase = 'steps'), 760),
      setTimeout(() => (enteredFromWelcome = false), 1300),
    );
  }

  async function finish() {
    if (finishing) return;
    finishing = true;
    error = null;
    try {
      if (persist) await completeOnboarding();
      onDone();
    } catch {
      error = 'Could not finish setup. Try again.';
      finishing = false;
    }
  }

  let stopDots: () => void = () => {};
  onMount(() => {
    if (phase === 'welcome') {
      playWelcome();
      stopDots = startDots();
    }
  });
  $effect(() => {
    if (phase === 'steps') stopDots();
  });
  onDestroy(() => {
    stopDots();
    cancelAnimationFrame(frame);
    timers.forEach(clearTimeout);
  });
</script>

<div class="first-run" bind:this={root}>
  {#if phase !== 'welcome'}
    <div class="fr-grid" class:from-welcome={enteredFromWelcome}>
      <aside class="fr-rail" aria-label="Setup progress">
        <div class="fr-brand"><BrandLogo decorative /></div>
        <ol class="fr-steps">
          {#each FIRST_RUN_STEPS as label, index (label)}
            {@const done = index < step}
            {@const current = index === step}
            <li
              class="fr-step"
              class:done
              class:current
              aria-current={current ? 'step' : undefined}
            >
              {#if done}<button
                  type="button"
                  onclick={() => go(index)}
                  aria-label={`Go back to ${label}`}
                ></button>{/if}
              <span class="ind"
                >{#if done}<Icon name="check" />{:else}{index + 1}{/if}</span
              >
              <span class="label">{label}</span>
            </li>
          {/each}
        </ol>
        {#if install.status !== 'idle'}
          <div class="fr-chip" class:error={install.status === 'error'} role="status">
            <p>
              <Icon
                name={install.status === 'done'
                  ? 'downloaded'
                  : install.status === 'error'
                    ? 'warning'
                    : 'downloading'}
              />
              {install.status === 'done'
                ? 'Models ready'
                : install.status === 'error'
                  ? 'Download stopped'
                  : 'Setting up models'}
              {#if install.status === 'running'}<span class="pct">{percent}%</span>{/if}
            </p>
            {#if install.status === 'running'}<div
                class="track"
                role="progressbar"
                aria-label="Model download"
                aria-valuemin="0"
                aria-valuemax="100"
                aria-valuenow={percent}
              >
                <span style:transform={`scaleX(${percent / 100})`}></span>
              </div>{/if}
            {#if install.status === 'error'}<button
                type="button"
                class="quiet"
                onclick={() => void startInstall(lastPlan)}>Try again</button
              >{/if}
          </div>
        {/if}
      </aside>
      <div class="fr-main">
        <div class="fr-scroll">
          {#if step === READY_STEP}
            <section class="fr-view" aria-labelledby="fr-title">
              <div class="ready-mark" aria-hidden="true"><Icon name="check" /></div>
              <h1 id="fr-title" tabindex="-1">Vesper is ready.</h1>
              <p class="lede">Hold your shortcut anywhere and talk.</p>
              <dl class="summary">
                {#each ['asr', 'rewriter'] as const as role (role)}
                  <div>
                    <dt>{MODEL_ROLE_LABELS[role]}</dt>
                    <dd>
                      {#if !settings}Loading…{:else if role === 'rewriter' && !settings.cleanupEnabled}Off{:else}{downloadTarget(
                          settings,
                          role,
                        ).name}{/if}
                    </dd>
                  </div>
                {/each}
                {#if settings}<div>
                    <dt>Your words</dt>
                    <dd>
                      {settings.cleanupEnabled
                        ? settings.cleanupPreset === 'balanced'
                          ? 'Polished'
                          : 'As I said it'
                        : 'Exactly as heard'}{words.length ? ` · ${words.join(', ')}` : ''}
                    </dd>
                  </div>
                  <div>
                    <dt>Typing into apps</dt>
                    <dd>
                      {settings.dictationOutput === 'paste'
                        ? 'On'
                        : 'Kept in Vesper, ready to copy'}
                    </dd>
                  </div>{/if}
                {#if shortcuts}<div>
                    <dt>Hold to talk</dt>
                    <dd><kbd>{shortcutLabel(shortcuts.hold)}</kbd></dd>
                  </div>{/if}
              </dl>
              <p class="note">
                Find your recordings in History. Change models and shortcuts in Settings.
              </p>
            </section>
          {:else}
            {@const copy = STEP_COPY[step]}
            <section class="fr-view" aria-labelledby="fr-title">
              <p class="step-count">Step {step + 1} of {FIRST_RUN_STEPS.length}</p>
              <h1 id="fr-title" tabindex="-1">{copy?.title}</h1>
              <p class="lede">{copy?.lede}</p>
              {#if step === 1}
                <FirstRunVoice
                  {language}
                  preview={!persist}
                  onLanguage={(next) => void chooseLanguage(next)}
                  onReady={(ready) => (voiceReady = ready)}
                />
              {:else if step === 2}
                {#if settings}<FirstRunModels
                    bind:this={models}
                    {settings}
                    {language}
                    started={install.status !== 'idle'}
                    preview={!persist}
                  />{:else}<p class="lede">Loading your settings…</p>{/if}
              {:else if step === 3 && settings}
                <FirstRunAccess
                  {settings}
                  preview={!persist}
                  {onSaveSettings}
                  onReady={(ready) => (accessReady = ready)}
                />
              {:else if step === 4 && settings}
                <FirstRunShortcut {settings} {onSaveSettings} />
              {:else if step === 5 && settings}
                <FirstRunWords
                  {settings}
                  preview={!persist}
                  {onSaveSettings}
                  onWords={(next) => (words = next)}
                />
              {:else}
                {#if step === 6 && settings}<FirstRunTry
                    {settings}
                    install={install.status}
                    {percent}
                    onRetry={() => void startInstall(lastPlan ?? undefined)}
                    onDone={(done) => (tried = done)}
                  />{/if}
              {/if}
            </section>
          {/if}
        </div>
        <footer class="fr-foot">
          {#if step >= 1 && step < READY_STEP}<button
              type="button"
              class="quiet"
              onclick={() => go(step - 1)}>Back</button
            >{/if}
          {#if error}<p class="error" role="alert">{error}</p>
          {:else if waiting}<p class="why" role="status">{waiting}</p>{/if}
          {#if step === READY_STEP}
            <button type="button" class="primary go" onclick={finish} disabled={finishing}
              >{finishing ? 'Finishing…' : 'Start dictating'}</button
            >
          {:else if step === 2 && install.status === 'idle'}
            <button
              type="button"
              class="primary go"
              disabled={!settings || Boolean(waiting)}
              onclick={async () => {
                await startInstall();
                if (install.status !== 'idle' && !error) go(3);
              }}>Download and set up</button
            >
          {:else}
            <button
              type="button"
              class="primary go"
              disabled={Boolean(waiting)}
              onclick={() => go(step + 1)}>{step === 6 ? 'Finish' : 'Continue'}</button
            >
          {/if}
        </footer>
      </div>
    </div>
  {/if}

  {#if flight}
    <div
      class="fr-flyer"
      bind:this={flyer}
      style:width={`${flight.width}px`}
      style:height={`${flight.height}px`}
      aria-hidden="true"
    >
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html artwork}
    </div>
  {/if}
  {#if phase === 'welcome' || phase === 'leaving'}
    <section
      class="hello"
      class:leaving={phase === 'leaving'}
      class:returning
      aria-labelledby="hello-title"
    >
      <canvas class="hello-dots" bind:this={dotCanvas} aria-hidden="true"></canvas>
      <div class="hello-mark">
        <svg class="reed" viewBox="0 0 320 100" preserveAspectRatio="none" aria-hidden="true"
          ><path bind:this={reedPath} d="" /></svg
        >
        <button
          type="button"
          class="hello-bird"
          class:in={birdIn}
          class:calling={chirping}
          bind:this={heroBird}
          aria-label="Make Vesper chirp"
          onclick={chirp}
          onpointerenter={chirp}
        >
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html artwork}
        </button>
      </div>
      <h1 id="hello-title" class="fx" class:in={textIn} style="--d:0ms">Welcome to Vesper</h1>
      <p class="fx sub" class:in={textIn} style="--d:110ms">
        Speak, and your words appear wherever you type. Everything stays on this Mac.
      </p>
      <button
        type="button"
        class="primary get-started fx"
        class:in={textIn}
        style="--d:240ms"
        onclick={leaveWelcome}>Get started</button
      >
    </section>
  {/if}
</div>

<style>
  .first-run {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: var(--color-bg);
    color: var(--color-ink);
    container: first-run / inline-size;
    overflow: hidden;
  }
  /* Welcome: one focal point and one signature motion. */
  .hello {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 40px 24px;
    background: var(--color-bg);
  }
  .hello-dots {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    color: var(--color-accent);
    pointer-events: none;
    animation: fade-in 2s var(--ease-out) both;
  }
  .hello > :not(.hello-dots) {
    position: relative;
  }
  .hello.returning {
    animation: hello-return 320ms var(--ease-out) both;
  }
  @keyframes hello-return {
    from {
      opacity: 0;
    }
  }
  .hello.leaving {
    pointer-events: none;
    animation: fade-out 620ms cubic-bezier(0.4, 0, 0.2, 1) 140ms both;
  }
  .hello.leaving .fx {
    animation: blur-out 360ms cubic-bezier(0.4, 0, 1, 1) both;
  }
  .hello.leaving .hello-bird {
    visibility: hidden;
  }
  .hello-mark {
    position: relative;
    width: 168px;
    height: 126px;
    margin-bottom: 40px;
  }
  .reed {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 420px;
    height: 110px;
    transform: translate(-50%, -50%);
    color: var(--color-accent);
    stroke: currentColor;
    stroke-width: 2.4;
    stroke-linecap: round;
    fill: none;
    overflow: visible;
  }
  .hello-bird {
    all: unset;
    cursor: pointer;
    position: absolute;
    inset: 0;
    color: var(--color-accent);
    opacity: 0;
    transform: scale(0.55);
    filter: blur(14px);
    transition:
      opacity 0.5s var(--ease-out),
      transform 1s var(--ease-spring),
      filter 0.8s var(--ease-out);
  }
  .hello-bird.in {
    opacity: 1;
    transform: none;
    filter: none;
  }
  .hello-bird:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 10px;
    border-radius: 16px;
  }
  .hello-bird :global(svg),
  .fr-flyer :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
    overflow: visible;
  }
  .hello-bird :global(.bd),
  .hello-bird :global(.dsh),
  .hello-bird :global(.lid),
  .fr-flyer :global(.bd),
  .fr-flyer :global(.dsh),
  .fr-flyer :global(.lid) {
    fill: currentColor;
  }
  .hello-bird :global(.dsh) {
    transform-box: fill-box;
    transform-origin: left center;
  }
  .hello-bird :global(.lid),
  .fr-flyer :global(.lid) {
    transform-box: fill-box;
    transform-origin: center;
    transform: scaleY(0);
  }
  .hello-bird :global(.lid) {
    animation: blink 4.2s ease-in-out infinite;
  }
  .hello-bird.calling :global(.dsh) {
    animation: call 0.9s ease-out both;
  }
  .hello h1 {
    font: 800 56px/1.04 var(--font-display);
    letter-spacing: -0.045em;
    margin: 0 0 14px;
  }
  .hello .sub {
    font-size: 18px;
    line-height: 1.5;
    color: var(--color-muted);
    margin: 0 0 34px;
    max-width: 30em;
    text-wrap: balance;
  }
  .get-started {
    min-height: 50px;
    padding: 0 30px;
    border-radius: var(--radius-pill);
    font-size: 16px;
  }
  .fx {
    opacity: 0;
    filter: blur(10px);
    transform: translateY(12px);
  }
  .fx.in {
    animation: blur-in 0.95s cubic-bezier(0.2, 0.8, 0.2, 1) var(--d) forwards;
  }
  .fr-flyer {
    position: absolute;
    left: 0;
    top: 0;
    z-index: 5;
    color: var(--color-accent);
    pointer-events: none;
    transform-origin: 0 0;
  }

  /* Steps: rail and content. */
  .fr-grid {
    display: grid;
    grid-template-columns: 264px 1fr;
    height: 100%;
  }
  .fr-grid.from-welcome .fr-rail {
    animation: rail-in 0.9s cubic-bezier(0.2, 0.8, 0.2, 1) 0.25s both;
  }
  .fr-grid.from-welcome .fr-main {
    animation: fade-in 0.8s cubic-bezier(0.2, 0.8, 0.2, 1) 0.35s both;
  }
  .fr-rail {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    padding: 30px 20px 22px;
    border-right: 1px solid var(--color-line);
    min-height: 0;
  }
  .fr-brand {
    padding: 0 6px;
  }
  .fr-steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .fr-step {
    position: relative;
    display: grid;
    grid-template-columns: 24px 1fr;
    gap: 12px;
    align-items: center;
    padding: 10px 12px;
    border: 1px solid transparent;
    border-radius: 12px;
    color: var(--color-muted);
  }
  .fr-step button {
    all: unset;
    position: absolute;
    inset: 0;
    border-radius: 12px;
    cursor: pointer;
  }
  .fr-step button:focus-visible {
    outline: 2px solid var(--color-focus);
  }
  .fr-step .ind {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 1.5px solid var(--color-control-edge);
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 600;
  }
  .fr-step .ind :global(svg) {
    width: 14px;
    height: 14px;
  }
  .fr-step.current {
    background: var(--color-raised);
    border-color: var(--color-line);
    color: var(--color-ink);
  }
  .fr-step.current .ind {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }
  .fr-step.done {
    color: var(--color-ink);
  }
  .fr-step.done .ind {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-on-accent);
  }
  .fr-chip {
    margin-top: auto;
    padding: 12px 14px;
    border: 1px solid var(--color-line);
    border-radius: 14px;
    background: var(--color-raised);
    font-size: 13px;
  }
  .fr-chip.error {
    border-color: var(--color-error);
  }
  .fr-chip p {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-weight: 500;
  }
  .fr-chip :global(svg) {
    width: 18px;
    height: 18px;
    color: var(--color-accent);
  }
  .fr-chip.error :global(svg) {
    color: var(--color-error);
  }
  .fr-chip .pct {
    margin-left: auto;
    color: var(--color-accent);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .fr-chip .track {
    height: 4px;
    margin-top: 10px;
    border-radius: 99px;
    background: var(--color-line);
    overflow: hidden;
  }
  .fr-chip .track span {
    display: block;
    height: 100%;
    background: var(--color-accent);
    transform-origin: left;
    transition: transform 180ms var(--ease);
  }
  .fr-foot .why {
    margin: 0 0 0 auto;
    font-size: 13px;
    color: var(--color-muted);
  }
  .fr-foot .why + .go {
    margin-left: 0;
  }
  .fr-main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .fr-scroll {
    flex: 1;
    overflow: auto;
    padding: 38px 56px 20px;
  }
  .fr-view {
    max-width: 800px;
  }
  .step-count {
    margin: 0 0 12px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-accent);
  }
  .fr-view h1 {
    font: 800 56px/1.02 var(--font-display);
    letter-spacing: -0.045em;
    margin: 0 0 14px;
    outline: none;
  }
  .lede {
    font-size: 17px;
    color: var(--color-muted);
    margin: 0 0 26px;
    max-width: 56ch;
  }
  .ready-mark {
    width: 84px;
    height: 84px;
    border-radius: 50%;
    background: var(--color-accent);
    color: var(--color-on-accent);
    display: grid;
    place-items: center;
    margin-bottom: 22px;
  }
  .ready-mark :global(svg) {
    width: 44px;
    height: 44px;
  }
  .summary {
    margin: 0;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
  }
  .summary div {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: var(--space-3);
    align-items: center;
    padding: 14px var(--space-6);
  }
  .summary div + div {
    border-top: 1px solid var(--color-line);
  }
  .summary dt {
    color: var(--color-muted);
  }
  .summary dd {
    margin: 0;
    font-weight: 500;
  }
  kbd {
    font: 600 14px var(--font-body);
    padding: 3px 10px;
    border-radius: 8px;
    border: 1px solid var(--color-control-edge);
    border-bottom-width: 2px;
  }
  .note {
    margin-top: var(--space-4);
    font-size: 13px;
    color: var(--color-muted);
  }
  .fr-foot {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: 16px 56px 22px;
    border-top: 1px solid var(--color-line);
  }
  .fr-foot .go {
    margin-left: auto;
  }
  .fr-foot .error {
    margin: 0;
    color: var(--color-error);
    font-size: 13px;
  }

  @container first-run (max-width: 760px) {
    .fr-grid {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
    }
    .fr-rail {
      flex-direction: row;
      align-items: center;
      padding: 12px 16px;
      border-right: 0;
      border-bottom: 1px solid var(--color-line);
    }
    .fr-brand {
      width: 120px;
    }
    .fr-steps {
      flex-direction: row;
      margin-left: auto;
    }
    .fr-step {
      padding: 0;
      background: none !important;
      border: 0;
      display: block;
    }
    .fr-step .label {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
    }
    .fr-scroll {
      padding: 22px 18px 14px;
    }
    .fr-view h1,
    .hello h1 {
      font-size: 36px;
    }
    .fr-foot {
      padding: 12px 18px 16px;
    }
  }
  @keyframes blur-in {
    to {
      opacity: 1;
      filter: blur(0);
      transform: none;
    }
  }
  @keyframes blur-out {
    from {
      opacity: 1;
      filter: blur(0);
      transform: none;
    }
    to {
      opacity: 0;
      filter: blur(6px);
      transform: translateY(-6px);
    }
  }
  @keyframes fade-out {
    to {
      opacity: 0;
    }
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }
  @keyframes rail-in {
    from {
      opacity: 0;
      transform: translateX(-20px);
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
  @keyframes call {
    0% {
      opacity: 0.1;
      transform: scale(0.4);
    }
    45% {
      opacity: 1;
      transform: scale(1.25);
    }
    100% {
      opacity: 1;
      transform: scale(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fx {
      opacity: 1;
      filter: none;
      transform: none;
    }
    .fx.in,
    .hello.returning,
    .hello.leaving,
    .hello.leaving .fx,
    .fr-grid.from-welcome .fr-rail,
    .fr-grid.from-welcome .fr-main,
    .hello-bird :global(.lid),
    .hello-bird.calling :global(.dsh) {
      animation: none;
    }
    .hello-bird {
      opacity: 1;
      transform: none;
      filter: none;
      transition: none;
    }
  }
</style>
