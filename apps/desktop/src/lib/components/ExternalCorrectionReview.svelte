<script lang="ts">
  import { untrack } from 'svelte';
  import {
    getExternalCorrectionReviews,
    rememberExternalCorrection,
    dismissExternalCorrection,
    type ExternalCorrectionReview,
    type PersonalizationProfile,
  } from '../api/personalization';
  let {
    revision,
    disabled,
    visible,
    onRemember,
  }: {
    revision: number;
    disabled: boolean;
    visible: boolean;
    onRemember: (profile: PersonalizationProfile) => void;
  } = $props();
  let reviews = $state<ExternalCorrectionReview[]>([]);
  let busy = $state(false);
  let error = $state('');
  let message = $state('');
  async function refresh() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      reviews = (await getExternalCorrectionReviews()) ?? [];
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  async function remember(
    review: ExternalCorrectionReview,
    pair: ExternalCorrectionReview['pairs'][number],
  ) {
    if (busy || disabled) return;
    busy = true;
    error = '';
    message = '';
    try {
      const profile = await rememberExternalCorrection(review.token, pair, revision);
      reviews = reviews.filter((item) => item.token !== review.token);
      onRemember(profile);
      message = `Remembered “${pair.intended}” for ${review.project || 'shared vocabulary'}. Remove it from your vocabulary to undo.`;
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  async function dismiss(review: ExternalCorrectionReview) {
    if (busy || disabled) return;
    busy = true;
    error = '';
    message = '';
    try {
      await dismissExternalCorrection(review.token);
      reviews = reviews.filter((item) => item.token !== review.token);
      message = 'Correction dismissed.';
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  $effect(() => {
    if (visible) {
      void revision;
      untrack(() => {
        void refresh();
      });
    }
  });
</script>

<section aria-labelledby="external-review-heading" aria-busy={busy}>
  <h3 id="external-review-heading">Corrections from TextEdit</h3>
  <p class="note">
    When Review corrections in TextEdit is enabled, eligible changes after Vesper pastes appear
    here. Confirm a spelling to use it again. Other changes do not teach your vocabulary.
  </p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if message}<p class="note" role="status">{message}</p>{/if}
  <button type="button" class="secondary" disabled={busy || disabled} onclick={refresh}
    >Refresh corrections</button
  >
  {#if !reviews.length && !busy}<p class="note">No spelling corrections waiting for review.</p>{/if}
  {#each reviews as review (review.token)}
    <article>
      <p class="note">
        Project: {review.project || 'Shared'} · Language: {review.language === 'auto'
          ? 'Automatic'
          : review.language}
      </p>
      <details>
        <summary>Review the change</summary>
        <label for={`external-before-${review.token}`}>Inserted text</label>
        <textarea id={`external-before-${review.token}`} readonly rows="3" value={review.before}
        ></textarea>
        <label for={`external-after-${review.token}`}>Changed text</label>
        <textarea id={`external-after-${review.token}`} readonly rows="3" value={review.after}
        ></textarea>
      </details>
      {#each review.pairs as pair (JSON.stringify(pair))}
        <p class="spelling">{pair.heard} → {pair.intended}</p>
        <button
          type="button"
          class="secondary"
          disabled={busy || disabled}
          onclick={() => remember(review, pair)}>Remember {pair.intended}</button
        >
      {/each}
      <button
        type="button"
        class="secondary"
        disabled={busy || disabled}
        onclick={() => dismiss(review)}>Dismiss correction</button
      >
    </article>
  {/each}
</section>

<style>
  section,
  article {
    display: grid;
    gap: 0.75rem;
    margin-block-start: 1rem;
  }
  article {
    padding-block: 0.75rem;
  }
  details {
    display: grid;
    gap: 0.5rem;
  }
  textarea {
    width: 100%;
  }
  .spelling {
    overflow-wrap: anywhere;
  }
</style>
