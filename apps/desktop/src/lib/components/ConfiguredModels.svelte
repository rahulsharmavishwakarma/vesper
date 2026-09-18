<script lang="ts">
  import Icon from './Icon.svelte';
  import { MODEL_ROLE_LABELS, type AppConfig } from '../config';
  import { downloadTarget } from '../modelDownloads';

  let { settings, onManage }: { settings: AppConfig | null; onManage: () => void } = $props();
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
        <dd>{settings ? downloadTarget(settings, role).name : 'Loading preferences…'}</dd>
      </div>
    {/each}
  </dl>
  <button type="button" class="quiet manage-models" disabled={!settings} onclick={onManage}
    >Manage models <Icon name="arrow" /></button
  >
</div>
