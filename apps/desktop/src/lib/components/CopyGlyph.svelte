<script lang="ts">
  let { copied }: { copied: boolean } = $props();
</script>

<svg
  class="copy-glyph"
  class:copied
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path class="sheets" d="M8 8h12v12H8zM16 8V4H4v12h4" />
  <path class="check" d="m5 12 4 4L19 6" pathLength="1" />
</svg>

<style>
  /* B02 Copy · the check is exposed only after clipboard success. */
  .copy-glyph {
    width: 20px;
    height: 20px;
    flex: 0 0 20px;
  }
  .sheets {
    transition: opacity 100ms linear;
  }
  .check {
    opacity: 0;
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    transition:
      stroke-dashoffset var(--duration-slide) var(--ease-slide),
      opacity 100ms linear;
  }
  .copied .sheets {
    opacity: 0;
  }
  .copied .check {
    opacity: 1;
    stroke-dashoffset: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .sheets,
    .check {
      transition: none;
    }
  }
</style>
