<script lang="ts">
  import Icon from '../src/lib/components/Icon.svelte';
  import CopyGlyph from '../src/lib/components/CopyGlyph.svelte';
  import Select from '../src/lib/components/Select.svelte';
  import TranscriptFeedback from '../src/lib/components/TranscriptFeedback.svelte';
  import DownloadProgress from '../src/lib/components/DownloadProgress.svelte';
  import ModelRemovalDialog from '../src/lib/components/ModelRemovalDialog.svelte';
  import ModelStatus from '../src/lib/components/ModelStatus.svelte';
  import { installPressFeedback } from '../src/lib/actions/pressFeedback';
  import { onMount } from 'svelte';
  import type { TranscriptFeedback as Feedback } from '../src/lib/api/dataset';
  import type { ModelCatalogInstallation } from '../src/lib/api/runtime';
  import type { DownloadOptionStatus } from '../src/lib/modelDownloads';
  let enabled = $state(true),
    value = $state('local'),
    copied = $state(true),
    feedback = $state<Feedback>(null);
  let removal: ReturnType<typeof ModelRemovalDialog>;
  const options = [
    { value: 'local', label: 'On this device' },
    { value: 'paused', label: 'Paused' },
  ];
  const model: ModelCatalogInstallation = {
    role: 'speech_recognition',
    modelId: 'synthetic',
    name: 'Synthetic model',
    selected: false,
    localFileCount: 1,
    removableBytes: 2048,
    downloadedBytes: 2048,
    quantization: null,
    artifactsInstalled: true,
    runtimeReady: true,
    removalError: null,
    ready: true,
    modelDirectory: 'synthetic',
    files: [{ name: 'synthetic.bin', installed: true, sizeBytes: 2048 }],
    supported: true,
    message: '',
  };
  const modelStates: DownloadOptionStatus[] = [
    { label: 'Downloaded', kind: 'downloaded', tone: 'ready' },
    { label: 'Q4 downloaded', detail: 'Q4', kind: 'downloaded', tone: 'ready' },
    { label: 'Not downloaded', kind: 'missing', tone: 'muted' },
    { label: 'Checking…', kind: 'busy', tone: 'muted' },
    { label: 'Downloading…', kind: 'busy', tone: 'muted' },
    { label: 'Incomplete download', kind: 'warning', tone: 'warning' },
    { label: 'Unavailable', kind: 'unavailable', tone: 'muted' },
    { label: 'Status unavailable', kind: 'unknown', tone: 'muted' },
  ];
  onMount(installPressFeedback);
</script>

<main class="state-preview">
  <h1>Small interactions, together.</h1>
  <p class="note">
    Synthetic controls · no audio, downloads or clipboard access. Hover and press specimens show
    forced visual states; the other controls are interactive.
  </p>
  <div class="state-grid">
    <section>
      <h2>Default</h2>
      <Select id="preview-select" label="Processing location" bind:value {options} />
    </section>
    <section>
      <h2>Hover</h2>
      <button class="primary is-hover"><Icon name="download" />Download model</button>
    </section>
    <section>
      <h2>Focus</h2>
      <button class="is-focus">Keyboard focus</button>
      <p class="note">Tab to any control to check actual focus.</p>
    </section>
    <section>
      <h2>Pressed</h2>
      <button class="primary is-pressed">Confirm choice</button>
    </section>
    <section>
      <h2>Disabled</h2>
      <label class="checkbox-field"
        ><input type="checkbox" disabled /><span>Unavailable preference</span></label
      ><button disabled>Download model</button>
    </section>
    <section>
      <h2>Loading</h2>
      <button aria-busy="true" disabled>Saving…</button><DownloadProgress
        progress={{
          phase: 'verifying',
          fileIndex: 1,
          fileCount: 1,
          fileName: 'synthetic.bin',
          completedBytes: 64,
          totalBytes: 64,
        }}
      />
    </section>
    <section>
      <h2>Error</h2>
      <label for="preview-invalid">Backup folder</label><input
        id="preview-invalid"
        aria-invalid="true"
        aria-describedby="preview-error"
        value="Unavailable folder"
      />
      <p id="preview-error" class="error">Choose an available folder.</p>
    </section>
    <section>
      <h2>Success</h2>
      <button class="quiet" onclick={() => (copied = !copied)}
        ><CopyGlyph {copied} />{copied ? 'Copied' : 'Preview copy feedback'}</button
      ><label class="checkbox-field"
        ><input type="checkbox" bind:checked={enabled} /><span>Use this preference</span></label
      >
    </section>
  </div>
  <h2>Model status icons</h2>
  <div class="state-grid model-state-preview">
    {#each modelStates as status (status.label)}<section>
        <p class="note">{status.label}</p>
        <ModelStatus {status} />
      </section>{/each}
  </div>
  <TranscriptFeedback
    value={feedback}
    onSave={async (value) => {
      feedback = value;
    }}
  />
  <details>
    <summary>Disclosure details</summary>
    <p>The arrow changes direction and the content stays keyboard accessible.</p>
  </details>
  <button onclick={() => removal.confirmRemoval(model)}
    ><Icon name="trash" />Preview removal dialog</button
  >
  <ModelRemovalDialog
    bind:this={removal}
    disabled={false}
    onRemove={async () => ({ removedBytes: 0, removedFiles: 0 })}
  />
</main>

<style>
  .state-preview {
    max-width: 960px;
    margin: auto;
    padding: var(--space-8) var(--space-5);
  }
  .state-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
  }
  .state-grid section {
    padding: var(--space-5);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-md);
    background: var(--color-panel);
  }
  h2 {
    margin-bottom: var(--space-4);
  }
  .is-hover {
    background-color: var(--color-accent-hover);
  }
  .is-focus {
    outline: 2px solid var(--color-focus);
    outline-offset: 3px;
  }
  .is-pressed {
    transform: translateY(1px) scale(0.985);
  }
  @media (max-width: 560px) {
    .state-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .is-pressed {
      transform: none;
    }
  }
</style>
