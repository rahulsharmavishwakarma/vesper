<script lang="ts">
  import type { SpeakerTimeline } from '../api/dataset';
  let { timeline }: { timeline: SpeakerTimeline | null | undefined } = $props();
  const stamp = (ms: number) =>
    `${Math.floor(ms / 60000)}:${((ms % 60000) / 1000).toFixed(2).padStart(5, '0')}`;
</script>

{#if timeline}
  <details class="original-transcript recording-disclosure">
    <summary>Speaker timeline</summary>
    <p class="note">
      Speaker numbers apply to this recording only. Overlapping ranges indicate simultaneous speech.
      Words have not been assigned to speakers.
    </p>
    {#if timeline.segments.length === 0}
      <p>No speaker activity detected.</p>
    {:else}
      <div class="speaker-intervals">
        <table>
          <caption class="note">Estimated speaker activity</caption>
          <thead
            ><tr><th scope="col">Speaker</th><th scope="col">Start</th><th scope="col">End</th></tr
            ></thead
          >
          <tbody
            >{#each timeline.segments.slice(0, 200) as segment, index (index)}
              <tr
                ><td>Speaker {segment.speaker}</td><td>{stamp(segment.startMs)}</td><td
                  >{stamp(segment.endMs)}</td
                ></tr
              >
            {/each}</tbody
          >
        </table>
      </div>
      {#if timeline.segments.length > 200}<p class="note">
          Showing the first 200 intervals. Dataset exports include the complete timeline.
        </p>{/if}
    {/if}
  </details>
{/if}

<style>
  .speaker-intervals {
    max-height: 20rem;
    overflow: auto;
  }
  table {
    width: 100%;
    text-align: left;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    padding: 0.35rem 0.5rem;
  }
</style>
