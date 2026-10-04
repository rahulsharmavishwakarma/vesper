<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import type { ModelCatalogInstallation } from '../api/runtime';
  import { downloadBytes } from '../modelDownloads';
  import { MODEL_PRESETS } from '../modelPresets';

  let {
    disabled,
    onRemove,
  }: {
    disabled: boolean;
    onRemove: (
      model: ModelCatalogInstallation,
    ) => Promise<{ removedBytes: number; removedFiles: number }>;
  } = $props();
  let dialog: HTMLDialogElement;
  let cancelButton = $state<HTMLButtonElement>();
  let selected = $state<ModelCatalogInstallation | null>(null);
  let removing = $state(false);
  let error = $state<string | null>(null);
  const locked = $derived(disabled || removing);
  const precision = (model: ModelCatalogInstallation) =>
    model.quantization?.slice(0, 2).toUpperCase() ??
    MODEL_PRESETS.find((preset) => preset.profile.id === model.modelId)?.precision ??
    'Model weights';
  export async function confirmRemoval(model: ModelCatalogInstallation) {
    if (locked || model.removalError || !model.localFileCount) return;
    selected = model;
    error = null;
    await tick();
    dialog.showModal();
    cancelButton?.focus();
  }
  function close() {
    if (!removing) dialog.close();
  }
  async function remove() {
    if (!selected || locked) return;
    const model = selected;
    removing = true;
    error = null;
    try {
      await onRemove(model);
      dialog.close();
      selected = null;
      await tick();
      const returnTo =
        model.role === 'audio_frontend'
          ? 'models-noise-reduction'
          : model.role === 'speech_recognition'
            ? 'models-asr-model'
            : model.quantization
              ? 'models-cleanup-precision'
              : 'models-cleanup-model';
      document.getElementById(returnTo)?.focus({ preventScroll: true });
    } catch (cause) {
      error = `Could not remove these files. ${cause instanceof Error ? cause.message : String(cause)}`;
    } finally {
      removing = false;
    }
  }
</script>

<dialog
  class="remove-dialog"
  bind:this={dialog}
  aria-labelledby="remove-model-heading"
  aria-describedby="remove-model-description"
  oncancel={(event) => {
    if (removing) event.preventDefault();
  }}
  onclose={() => {
    if (!removing) selected = null;
  }}
>
  {#if selected}
    <span class="eyebrow">Model storage</span>
    <h2 id="remove-model-heading">Remove this download?</h2>
    <p class="remove-model-name">{selected.name} <span>· {precision(selected)}</span></p>
    <p id="remove-model-description">
      This removes the model from Vesper. Your recordings and transcripts stay saved. You can
      install this model again.
    </p>
    {#if selected.selected}<p class="selection-warning">
        Your selection stays saved, but this model will be unavailable until you install it again.
      </p>{/if}
    {#if selected.downloadedBytes > selected.removableBytes}<p class="note">
        Shared model files, including the Hugging Face cache, will be kept.
      </p>{/if}
    {#if selected.removableBytes > 0}<p class="note">
        {downloadBytes(selected.removableBytes)} of unshared model files will be deleted. Actual disk
        space released depends on your filesystem.
      </p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="remove-actions">
      <button type="button" bind:this={cancelButton} disabled={removing} onclick={close}
        >Keep model</button
      ><button
        type="button"
        class="remove-confirm"
        disabled={locked}
        aria-busy={removing}
        onclick={remove}><Icon name="trash" />{removing ? 'Removing…' : 'Remove download'}</button
      >
    </div>
  {/if}
</dialog>

<style>
  .remove-dialog {
    width: min(460px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    margin: auto;
    padding: var(--space-6);
    color: var(--color-ink);
    background: var(--color-raised);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
  }
  .remove-dialog[open] {
    animation: remove-confirm-enter 180ms var(--ease) both;
  }
  .remove-dialog::backdrop {
    background: var(--color-overlay);
  }
  .remove-dialog h2 {
    font-size: 21px;
    margin: var(--space-3) 0 var(--space-5);
  }
  .remove-dialog p {
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .remove-model-name {
    font-weight: 500;
  }
  .remove-model-name span,
  #remove-model-description {
    color: var(--color-muted);
  }
  .selection-warning {
    padding-left: var(--space-3);
    border-left: 2px solid var(--color-warning);
  }
  .remove-actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-6);
  }
  .remove-confirm:hover:enabled {
    background: var(--color-error-tint);
  }
  .remove-confirm {
    color: var(--color-error);
    border-color: var(--color-error);
    background: var(--color-transparent);
  }
  @keyframes remove-confirm-enter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (max-width: 560px) {
    .remove-actions {
      flex-direction: column-reverse;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
