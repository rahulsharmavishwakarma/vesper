<script lang="ts">
  import Icon from './Icon.svelte';
  import { MODEL_ROLE_LABELS, type AppConfig } from '../config';
  import { downloadTarget } from '../modelDownloads';
  import { Settled } from '../settled.svelte';

  let { settings, onManage }: { settings: AppConfig | null; onManage: () => void } = $props();
  // Settings usually arrive within a moment; only a slow load says so.
  const loading = new Settled(
    () => settings === null,
    (value) => value,
  );
</script>

<div class="configured-models" role="group" aria-label="Configured models">
  <dl>
    {#each ['asr', 'rewriter'] as const as role (role)}
      <div>
        <dt>
          {MODEL_ROLE_LABELS[
            role
          ]}{#if role === 'rewriter' && settings && !settings.cleanupEnabled}<span
              class="writing-off"
            >
              · Off</span
            >{/if}
        </dt>
        <dd>
          {settings
            ? downloadTarget(settings, role).name
            : loading.value
              ? 'Loading preferences…'
              : '\u00a0'}
        </dd>
      </div>
    {/each}
  </dl>
  <button type="button" class="quiet manage-models" disabled={!settings} onclick={onManage}
    >Manage models <Icon name="arrow" /></button
  >
</div>
