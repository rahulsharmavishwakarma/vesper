<script lang="ts">
  import { selectionHighlight } from '../actions/selectionHighlight';
  import type { SessionRecord } from '../api/dataset';
  import { TYPING_WPM, durationParts, formatDuration, historyActivity } from '../history';
  let {
    records,
    selectedDay,
    onSelectDay,
  }: { records: SessionRecord[]; selectedDay: string; onSelectDay: (day: string) => void } =
    $props();
  let range = $state<7 | 30>(7);
  const activity = $derived(historyActivity(records, range));
  const peak = $derived(Math.max(1, ...activity.days.map((day) => day.count)));
  let inspectedDay = $state('');
  const points = $derived(
    activity.days.map((day, index) => ({
      x: ((index + 0.5) / range) * 600,
      y: 96 - (day.count / peak) * 88,
    })),
  );
  const line = $derived.by(() => {
    const slopes = points
      .slice(1)
      .map((point, index) => (point.y - points[index].y) / (point.x - points[index].x));
    // Flatten extrema and flat runs; bounded tangents keep the curve within each day's values.
    const tangents = points.map((_, index) => {
      if (index === 0) return slopes[0];
      if (index === points.length - 1) return slopes[index - 1];
      const before = slopes[index - 1];
      const after = slopes[index];
      return before * after <= 0 ? 0 : (2 * before * after) / (before + after);
    });
    return points
      .map((point, index) => {
        if (index === 0) return `M${point.x},${point.y}`;
        const previous = points[index - 1];
        const handle = (point.x - previous.x) / 3;
        return `C${previous.x + handle},${previous.y + tangents[index - 1] * handle} ${point.x - handle},${point.y - tangents[index] * handle} ${point.x},${point.y}`;
      })
      .join(' ');
  });
  const inspected = $derived(activity.days.find((day) => day.key === inspectedDay));
  const highlighted = $derived(activity.days.find((day) => day.key === selectedDay));
  function moveDay(event: KeyboardEvent, index: number) {
    const next =
      event.key === 'ArrowRight'
        ? Math.min(range - 1, index + 1)
        : event.key === 'ArrowLeft'
          ? Math.max(0, index - 1)
          : event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? range - 1
              : null;
    if (next === null) return;
    event.preventDefault();
    const parent = (event.currentTarget as HTMLElement).parentElement;
    const buttons = parent?.querySelectorAll<HTMLButtonElement>('button');
    buttons?.forEach((button, i) => {
      button.tabIndex = i === next ? 0 : -1;
    });
    buttons?.[next]?.focus();
  }
</script>

<section class="history-stats" aria-labelledby="history-stats-heading">
  <div class="section-heading">
    <h2 id="history-stats-heading">Your recording rhythm</h2>
    <div
      class="stats-range"
      use:selectionHighlight={range}
      role="group"
      aria-label="Activity period"
    >
      {#each [7, 30] as days (days)}<button
          type="button"
          aria-pressed={range === days}
          onclick={() => {
            range = days as 7 | 30;
            onSelectDay('');
            inspectedDay = '';
          }}>{days} days</button
        >{/each}
    </div>
  </div>
  {#key range}<dl class="history-metrics">
      <div>
        <dt>Time saved</dt>
        <dd>
          {#each durationParts(activity.savedMs) as [value, unit] (unit)}<span class="duration-part"
              >{value}<span class="unit">{unit}</span></span
            >{/each}
        </dd>
      </div>
      <div>
        <dt>Time recorded</dt>
        <dd>
          {#each durationParts(activity.recordedMs) as [value, unit] (unit)}<span
              class="duration-part">{value}<span class="unit">{unit}</span></span
            >{/each}
        </dd>
      </div>
      <div>
        <dt>Words</dt>
        <dd>{activity.words.toLocaleString()}</dd>
      </div>
      <div>
        <dt>Recordings</dt>
        <dd>{activity.recordings.toLocaleString()}</dd>
      </div>
    </dl>{/key}
  <div class="activity-chart-heading">
    <span>Recordings per day</span><span class="activity-readout"
      >{#if inspected}{inspected.label} · {inspected.count}
        {inspected.count === 1 ? 'recording' : 'recordings'} · {formatDuration(inspected.savedMs)} saved{:else}Last
        {range} days{/if}</span
    >
  </div>
  <div class="activity-plot">
    <div class="activity-scale" aria-hidden="true"><span>{peak}</span><span>0</span></div>
    <div
      class="activity-chart"
      role="group"
      aria-label={`Recordings over the last ${range} days. Use arrow keys to explore, and Enter to filter by day.`}
    >
      {#key range}<svg viewBox="0 0 600 100" preserveAspectRatio="none" aria-hidden="true">
          <path class="activity-gridline" d="M0,8H600 M0,96H600" />
        </svg>
        <svg
          class="activity-ink"
          viewBox="0 0 600 100"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <path
            class="activity-area"
            d={`${line} L${points[range - 1].x},96 L${points[0].x},96 Z`}
          />
          <path class="activity-line" d={line} />
        </svg>{/key}
      {#each activity.days as day, index (day.key)}
        <button
          class="activity-day"
          type="button"
          tabindex={selectedDay ? (selectedDay === day.key ? 0 : -1) : index === range - 1 ? 0 : -1}
          aria-pressed={selectedDay === day.key}
          aria-label={`${day.label}: ${day.count} ${day.count === 1 ? 'recording' : 'recordings'}. Filter this day.`}
          title={`${day.label} · ${day.count} ${day.count === 1 ? 'recording' : 'recordings'}`}
          onclick={() => onSelectDay(selectedDay === day.key ? '' : day.key)}
          onkeydown={(event) => moveDay(event, index)}
          onpointerenter={() => {
            inspectedDay = day.key;
          }}
          onpointerleave={() => {
            inspectedDay = '';
          }}
          onfocus={() => {
            inspectedDay = day.key;
          }}
          onblur={() => {
            inspectedDay = '';
          }}
          style:left={`${(index / range) * 100}%`}
          style:width={`${100 / range}%`}
          style:--point-y={`${points[index].y}%`}><span class="activity-point"></span></button
        >
      {/each}
    </div>
  </div>
  <div class="activity-axis" aria-hidden="true">
    <span>{activity.days[0].label}</span><span
      >{activity.days[Math.floor((range - 1) / 2)].label}</span
    ><span>{activity.days[range - 1].label}</span>
  </div>
  <dl class="history-insights">
    <div>
      <dt>Active days</dt>
      <dd>{activity.activeDays}<span class="unit">of {range}</span></dd>
    </div>
    <div>
      <dt>Speaking pace</dt>
      <dd>
        {#if activity.pace}{Math.round(activity.pace).toLocaleString()}<span class="unit"
            >words a minute</span
          >{:else}—{/if}
      </dd>
    </div>
    <div>
      <dt>Words per recording</dt>
      <dd>
        {activity.withText ? Math.round(activity.averageWords).toLocaleString() : '—'}
      </dd>
    </div>
  </dl>
  <p class="note activity-explanation" aria-live="polite">
    {#if highlighted}{highlighted.label} · {highlighted.count}
      {highlighted.count === 1 ? 'recording' : 'recordings'}.
      <button type="button" class="inline-link" onclick={() => onSelectDay('')}
        >Show all dates</button
      >{:else if activity.recordings}Time saved compares typing at {TYPING_WPM} words a minute. Select
      a day to filter recordings.{:else}No recordings in the last {range} days.{/if}
  </p>
</section>
