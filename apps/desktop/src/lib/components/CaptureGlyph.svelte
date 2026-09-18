<script lang="ts">
  let { recording }: { recording: boolean } = $props();
</script>

<span class="capture-glyph" class:recording aria-hidden="true"><span></span></span>

<style>
  /* B01 Capture · shared geometry keeps the same element through start/stop. */
  .capture-glyph {
    display: grid;
    place-items: center;
    position: relative;
    width: 20px;
    height: 20px;
    flex: 0 0 20px;
  }
  .capture-glyph > span {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: currentColor;
    transition:
      border-radius var(--duration-slide) var(--ease-slide),
      transform var(--duration-slide) var(--ease-slide);
  }
  .recording > span {
    border-radius: 2px;
    transform: scale(0.75);
  }
  @media (prefers-reduced-motion: reduce) {
    .capture-glyph > span {
      transition: none;
    }
  }
</style>
