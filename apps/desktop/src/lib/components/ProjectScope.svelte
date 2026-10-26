<script lang="ts">
  import type { ProjectSelection } from '../api/personalization';
  let { selection }: { selection?: ProjectSelection | null } = $props();
</script>

{#if selection}
  <p class="note">
    Project: {selection.project || 'Shared'}
    {#if selection.source === 'context_unavailable'}
      · Project context unavailable.
    {:else if selection.source === 'ambiguous'}
      · Conflicting project rules.
    {:else if selection.source === 'application'}
      · Selected by application.
    {:else if selection.source === 'folder'}
      · Selected by folder.
    {:else if selection.source === 'workspace'}
      · Detected workspace.
    {/if}
  </p>
{/if}
