<script lang="ts">
  import { tick, untrack } from 'svelte';
  import {
    getPersonalization,
    savePersonalization,
    getSelectedAdapters,
    getAutomaticLearning,
    forgetAutomaticLearning,
    type AutomaticLearningState,
    type PersonalizationProfile,
    type OutputIntent,
  } from '../api/personalization';
  import type { Language } from '../config';
  import Select from './Select.svelte';
  let {
    desktop,
    disabled = false,
    visible = true,
    settingsRevision = '',
  }: {
    desktop: boolean;
    disabled?: boolean;
    visible?: boolean;
    settingsRevision?: string;
  } = $props();
  let profile = $state<PersonalizationProfile | null>(null);
  let savedProfile = $state<PersonalizationProfile | null>(null);
  let learning = $state<AutomaticLearningState | null>(null);
  let pending = $state<'loading' | 'saving' | null>(null);
  let speechBiasSupported = $state(false);
  let checkingSpeech = $state(false);
  let speechCheckFailed = $state(false);
  let error = $state('');
  let feedback = $state('');
  const dirty = $derived(
    profile !== null && JSON.stringify(profile) !== JSON.stringify(savedProfile),
  );
  const controlsDisabled = $derived(pending !== null || disabled || !visible);
  const outputOptions: { value: OutputIntent; label: string }[] = [
    { value: 'verbatim', label: 'Dictated wording' },
    { value: 'note', label: 'Note' },
    { value: 'message', label: 'Message' },
    { value: 'technical', label: 'Technical dictation' },
  ];
  const languageOptions: { value: Language; label: string }[] = [
    { value: 'auto', label: 'Any language' },
    { value: 'en', label: 'English' },
    { value: 'hi', label: 'Hindi' },
    { value: 'fr', label: 'French' },
  ];
  async function load() {
    if (!desktop || pending) return;
    pending = 'loading';
    error = '';
    feedback = '';
    try {
      profile = await getPersonalization();
      savedProfile = $state.snapshot(profile);
      learning = await getAutomaticLearning();
    } catch (e) {
      error = String(e);
    } finally {
      pending = null;
    }
  }
  async function save() {
    if (!profile || controlsDisabled || !dirty) return;
    pending = 'saving';
    error = '';
    feedback = '';
    try {
      profile = await savePersonalization($state.snapshot(profile));
      savedProfile = $state.snapshot(profile);
      learning = await getAutomaticLearning();
      feedback = 'Saved for your next recording.';
    } catch (e) {
      error = String(e);
    } finally {
      pending = null;
    }
  }
  async function forgetLearning() {
    if (!profile || controlsDisabled || dirty) return;
    pending = 'saving';
    error = '';
    feedback = '';
    try {
      profile = await forgetAutomaticLearning(profile.revision);
      savedProfile = $state.snapshot(profile);
      learning = await getAutomaticLearning();
      feedback = 'Automatic learning forgotten.';
    } catch (cause) {
      error = String(cause);
    } finally {
      pending = null;
    }
  }
  function discard() {
    if (!savedProfile || controlsDisabled) return;
    profile = $state.snapshot(savedProfile);
    error = '';
    feedback = 'Changes discarded.';
  }
  async function addWord() {
    if (!profile || controlsDisabled || profile.vocabulary.length >= 128) return;
    const id = crypto.randomUUID();
    profile.vocabulary.push({
      id,
      phrase: '',
      language: 'auto',
      context: '',
      enabled: true,
      aliases: [],
      project: profile.activeProject,
      pronunciation: false,
      sources: [],
      learnedAt: '',
      automaticSource: null,
      usageCount: 0,
    });
    await tick();
    document.getElementById(`word-${id}`)?.focus();
  }
  async function removeWord(id: string) {
    if (!profile || controlsDisabled) return;
    profile.vocabulary = profile.vocabulary.filter((word) => word.id !== id);
    await tick();
    document.getElementById('add-vocabulary')?.focus();
  }
  $effect(() => {
    // Refresh model support when saved model settings change; preserve the local draft on navigation.
    if (!desktop || !visible) return;
    void settingsRevision;
    let current = true;
    checkingSpeech = true;
    speechCheckFailed = false;
    getSelectedAdapters()
      .then((adapters) => {
        if (current) speechBiasSupported = adapters.speech.capabilities.contextualPhrases;
      })
      .catch(() => {
        if (current) {
          speechBiasSupported = false;
          speechCheckFailed = true;
        }
      })
      .finally(() => {
        if (current) checkingSpeech = false;
      });
    return () => {
      current = false;
    };
  });
  $effect(() => {
    if (!desktop || !visible) return;
    // History may have remembered new corrections. Refresh clean forms on return,
    // but keep unsaved edits and let the profile revision guard resolve conflicts.
    untrack(() => {
      if (!dirty) void load();
    });
  });
</script>

<section
  aria-labelledby="personalization-heading"
  class="settings-section personalization"
  hidden={!visible}
>
  <header class="personalization-heading">
    <h2 id="personalization-heading">Your vocabulary and writing</h2>
    <p class="note">
      Vesper automatically learns eligible spellings from your edits in Your text and uses dictation
      usage to rank saved words. Learning stays on this device, even when audio collection is off.
      Edits in other apps are not observed. You can remove or forget learned words below.
    </p>
  </header>
  {#if !desktop}
    <p class="note">Personal preferences are available in the desktop app.</p>
  {:else}
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
      aria-busy={pending !== null}
    >
      {#if profile}
        <fieldset disabled={controlsDisabled}>
          <legend class="visually-hidden">Personal preferences</legend>
          <div class="preference-fields">
            <div class="field">
              <label for="personal-project"
                >Current project <span class="optional">(optional)</span></label
              >
              <input
                id="personal-project"
                bind:value={profile.activeProject}
                maxlength="64"
                placeholder="For example, work or a project name"
                aria-describedby="personal-project-help"
              />
              <p id="personal-project-help" class="note">
                Choose a project to use its saved words and examples.
              </p>
            </div>
            <div class="field">
              <label for="personal-intent">Output style</label>
              <Select
                id="personal-intent"
                bind:value={profile.outputIntent}
                options={outputOptions}
                disabled={controlsDisabled}
                describedBy="personal-intent-help"
              />
              <p id="personal-intent-help" class="note">
                Guides punctuation and layout while preserving your meaning.
              </p>
            </div>
          </div>
          <div class="preference-toggles">
            <div class="setting-toggle">
              <label class="checkbox-field" for="personal-corrections"
                ><input
                  id="personal-corrections"
                  type="checkbox"
                  bind:checked={profile.applyCorrections}
                  aria-describedby="personal-corrections-help"
                /><span>Apply my saved corrections</span></label
              >
              <p id="personal-corrections-help" class="note checkbox-help">
                Use saved spellings and pronunciation corrections, even when the writing model is
                off.
              </p>
            </div>
            <div class="setting-toggle">
              <label class="checkbox-field" for="personal-remember-intent"
                ><input
                  id="personal-remember-intent"
                  type="checkbox"
                  bind:checked={profile.useRememberedIntent}
                  aria-describedby="personal-remember-intent-help"
                /><span>Use the output style from similar reviewed examples</span></label
              >
              <p id="personal-remember-intent-help" class="note checkbox-help">
                Falls back to your selected style when there is no clear match.
              </p>
            </div>
            <div class="setting-toggle">
              <label class="checkbox-field" for="personal-fillers"
                ><input
                  id="personal-fillers"
                  type="checkbox"
                  bind:checked={profile.preserveFillers}
                /><span>Keep fillers and repetition</span></label
              >
            </div>
            <div class="setting-toggle">
              <label class="checkbox-field" for="personal-speech-hints"
                ><input
                  id="personal-speech-hints"
                  type="checkbox"
                  bind:checked={profile.speechBiasEnabled}
                  disabled={!speechBiasSupported || checkingSpeech}
                  aria-describedby="personal-speech-hints-help"
                /><span>Use my vocabulary during speech recognition</span></label
              >
              <p id="personal-speech-hints-help" class="note checkbox-help">
                {checkingSpeech
                  ? 'Checking speech model support…'
                  : speechCheckFailed
                    ? 'Could not check speech model support. Your saved preference is unchanged.'
                    : speechBiasSupported
                      ? 'May help recurring names, but can slow processing or affect other words.'
                      : 'Unavailable with the selected speech model. Saved corrections can still apply afterward.'}
              </p>
            </div>
          </div>
          <div class="preference-group">
            <div class="group-heading">
              <h3>Vocabulary</h3>
              <span class="entry-count">{profile.vocabulary.length} / 128</span>
            </div>
            {#if !profile.vocabulary.length}<p class="note">
                Add names or terms you want Vesper to get right.
              </p>{/if}
            {#each profile.vocabulary as word, index (word.id)}
              <div class="word-entry">
                <div class="entry-heading">
                  <h4>Word {index + 1}</h4>
                  <button
                    type="button"
                    class="quiet"
                    aria-label={`Remove word ${index + 1}`}
                    onclick={() => {
                      void removeWord(word.id);
                    }}>Remove</button
                  >
                </div>
                <div class="preference-fields">
                  <div class="field">
                    <label for={`word-${word.id}`}>Preferred spelling</label><input
                      id={`word-${word.id}`}
                      bind:value={word.phrase}
                      oninput={() => {
                        word.sources = [];
                        word.automaticSource = null;
                      }}
                      placeholder="Project or person’s name"
                      maxlength="128"
                      required
                    />
                  </div>
                  <div class="field">
                    <label for={`word-language-${word.id}`}>Language</label><Select
                      id={`word-language-${word.id}`}
                      bind:value={word.language}
                      options={languageOptions}
                      disabled={controlsDisabled}
                      onChange={() => {
                        word.sources = [];
                        word.automaticSource = null;
                      }}
                    />
                  </div>
                </div>
                <details>
                  <summary>Context and pronunciation</summary>
                  <div class="preference-fields">
                    <div class="field">
                      <label for={`word-context-${word.id}`}
                        >Context words <span class="optional">(optional)</span></label
                      ><input
                        id={`word-context-${word.id}`}
                        bind:value={word.context}
                        oninput={() => {
                          word.automaticSource = null;
                          word.sources = [];
                        }}
                        maxlength="256"
                        placeholder="For example, repository deployment"
                      />
                    </div>
                    <div class="field">
                      <label for={`word-project-${word.id}`}
                        >Project <span class="optional">(optional)</span></label
                      ><input
                        id={`word-project-${word.id}`}
                        bind:value={word.project}
                        oninput={() => {
                          word.automaticSource = null;
                          word.sources = [];
                        }}
                        maxlength="64"
                      />
                    </div>
                  </div>
                  <div class="field">
                    <label for={`word-aliases-${word.id}`}>Heard as</label><textarea
                      id={`word-aliases-${word.id}`}
                      rows="2"
                      value={word.aliases.join('\n')}
                      oninput={(event) => {
                        word.aliases = event.currentTarget.value
                          .split('\n')
                          .map((s) => s.trim())
                          .filter(Boolean);
                        word.sources = [];
                        word.automaticSource = null;
                      }}
                      placeholder="A recurring mistaken spelling"
                      aria-describedby={`word-aliases-help-${word.id}`}></textarea>
                    <p id={`word-aliases-help-${word.id}`} class="note">
                      One phrase per line, up to 8. Context and project limit when this correction
                      applies.
                    </p>
                  </div>
                  <label class="checkbox-field" for={`word-pronunciation-${word.id}`}
                    ><input
                      id={`word-pronunciation-${word.id}`}
                      type="checkbox"
                      bind:checked={word.pronunciation}
                    /><span>This is a recurring pronunciation confusion</span></label
                  >
                </details>
                <div class="setting-toggle">
                  <label class="checkbox-field" for={`word-enabled-${word.id}`}
                    ><input
                      id={`word-enabled-${word.id}`}
                      type="checkbox"
                      bind:checked={word.enabled}
                    /><span>Use this spelling</span></label
                  >
                  <p class="note checkbox-help">
                    {word.automaticSource
                      ? 'Learned automatically from ordinary edits; not a verified audio transcript.'
                      : word.sources.length
                        ? `Confirmed from ${word.sources.length} reviewed correction${word.sources.length === 1 ? '' : 's'}.`
                        : 'Manually added.'}
                  </p>
                </div>
              </div>
            {/each}
            <button
              id="add-vocabulary"
              type="button"
              onclick={() => {
                void addWord();
              }}
              disabled={profile.vocabulary.length >= 128}>Add vocabulary</button
            >
          </div>
          {#if profile.examples.length}
            <div class="preference-group" id="remembered-examples">
              <div class="group-heading">
                <h3>Remembered examples</h3>
                <span class="entry-count">{profile.examples.length}</span>
              </div>
              <p class="note">
                Edit the wording in History. Save here to pause or remove an example.
              </p>
              {#each profile.examples as example, index (example.id)}
                <div class="example-entry">
                  <div class="entry-heading">
                    <h4>Example {index + 1}</h4>
                    <button
                      type="button"
                      class="quiet"
                      aria-label={`Remove example ${index + 1}`}
                      onclick={() => {
                        if (profile)
                          profile.examples = profile.examples.filter((e) => e.id !== example.id);
                      }}>Remove</button
                    >
                  </div>
                  <dl class="example-text">
                    <dt>Original</dt>
                    <dd>{example.input}</dd>
                    <dt>Reviewed</dt>
                    <dd>{example.output}</dd>
                  </dl>
                  <div class="preference-fields">
                    <div class="field">
                      <label for={`example-project-${example.id}`}
                        >Project <span class="optional">(optional)</span></label
                      ><input
                        id={`example-project-${example.id}`}
                        bind:value={example.project}
                        maxlength="64"
                      />
                    </div>
                    <div class="field">
                      <label for={`example-intent-${example.id}`}>Output style</label><Select
                        id={`example-intent-${example.id}`}
                        bind:value={example.intent}
                        options={outputOptions}
                        disabled={controlsDisabled}
                      />
                    </div>
                  </div>
                  <label class="checkbox-field" for={`example-enabled-${example.id}`}
                    ><input
                      id={`example-enabled-${example.id}`}
                      type="checkbox"
                      bind:checked={example.enabled}
                    /><span>Use this example</span></label
                  >
                </div>
              {/each}
            </div>
          {/if}
          {#if learning}
            <div class="preference-group">
              <h3>Automatic learning evidence</h3>
              <p class="note">
                {learning.candidates.filter((c) => c.state === 'active').length} active · {learning.candidates.filter(
                  (c) => c.state === 'provisional',
                ).length} awaiting spelling evidence · {learning.candidates.filter(
                  (c) => c.state === 'suspended',
                ).length} paused. Usage counts do not prove a spelling is correct.
              </p>
              <p class="note">
                {learning.events.filter((e) => e.retry === 'possible_repair').length} possible repair
                signals. These do not create replacement rules.
              </p>
              <details>
                <summary>Inspect recent candidates</summary>
                {#each learning.candidates
                  .slice()
                  .reverse()
                  .slice(0, 32) as candidate (candidate.id)}
                  <p>
                    <strong>{candidate.phrase}</strong>{candidate.alias
                      ? ` ← ${candidate.alias}`
                      : ''} · {candidate.state} · {candidate.support.length} independent edit sources<br
                    /><span class="note"
                      >{candidate.project || candidate.context || 'No project context'} · {candidate.reason}</span
                    >
                  </p>
                {/each}
                <p class="note">
                  Shows up to 32 recent candidates. Active entries can be edited, paused or removed
                  in Vocabulary.
                </p>
              </details>
              <button type="button" disabled={controlsDisabled || dirty} onclick={forgetLearning}
                >Forget automatic learning</button
              >
              <p class="note">
                Removes automatic spellings, candidates and usage evidence. Manually saved words and
                reviewed recordings remain. New recordings automatically teach fresh corrections.
              </p>
            </div>
          {/if}
        </fieldset>
      {/if}
      <div class="preferences-footer">
        <div class="preference-feedback" aria-live="polite">
          {#if error}<p class="error" role="alert">{error}</p>
          {:else if pending === 'loading'}<p class="note">Loading personal preferences…</p>
          {:else if pending === 'saving'}<p class="note">Saving personal preferences…</p>
          {:else if dirty}<p class="note">Personal preferences have unsaved changes.</p>
          {:else if feedback}<p class="success">{feedback}</p>
          {:else if profile}<p class="note">Personal preferences are up to date.</p>{/if}
        </div>
        <div class="form-actions">
          {#if profile}
            <button type="submit" class="primary" disabled={controlsDisabled || !dirty}
              >{pending === 'saving' ? 'Saving…' : 'Save preferences'}</button
            >
            <button type="button" onclick={discard} disabled={controlsDisabled || !dirty}
              >Discard changes</button
            >
            {#if error && !dirty}<button type="button" onclick={load} disabled={controlsDisabled}
                >Reload saved preferences</button
              >{/if}
          {:else if error}<button type="button" onclick={load} disabled={controlsDisabled}
              >Try again</button
            >{/if}
        </div>
      </div>
    </form>
  {/if}
</section>

<style>
  /* Hallmark · component: personal preferences · existing Vesper Studio tokens
   * pre-emit critique: P5 H4 E5 S5 R5 V4 */
  .personalization {
    margin-top: var(--space-6);
  }
  .personalization-heading {
    margin-bottom: var(--space-6);
  }
  .personalization-heading h2 {
    margin-bottom: var(--space-2);
  }
  .personalization-heading p {
    margin-bottom: 0;
  }
  .preference-fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
  }
  .preference-fields > .field {
    min-width: 0;
    margin-bottom: var(--space-5);
  }
  .field > .note {
    margin-bottom: 0;
  }
  .optional,
  .entry-count {
    color: var(--color-muted);
    font-weight: 400;
  }
  .preference-toggles {
    display: grid;
    gap: var(--space-4);
  }
  .preference-toggles .setting-toggle {
    margin-top: 0;
  }
  .preference-group {
    margin-top: var(--space-6);
    padding-top: var(--space-6);
    border-top: 1px solid var(--color-line);
  }
  .group-heading,
  .entry-heading {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .group-heading {
    margin-bottom: var(--space-3);
  }
  .group-heading h3,
  .entry-heading h4 {
    margin: 0;
  }
  .entry-count {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .entry-heading {
    margin-bottom: var(--space-3);
  }
  .entry-heading h4 {
    font-size: 12px;
    font-weight: 500;
  }
  .word-entry,
  .example-entry {
    padding: var(--space-4);
    margin-bottom: var(--space-4);
    background: var(--color-bg);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
  }
  .word-entry details {
    margin-top: 0;
    margin-bottom: var(--space-3);
  }
  .example-text {
    margin: 0 0 var(--space-4);
    font-size: 12px;
  }
  .example-text dt {
    color: var(--color-muted);
    margin-bottom: var(--space-1);
  }
  .example-text dd {
    margin: 0 0 var(--space-3);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    line-height: 1.6;
  }
  .preferences-footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    margin-top: var(--space-6);
    padding-top: var(--space-5);
    border-top: 1px solid var(--color-line);
  }
  .preference-feedback {
    flex: 1 1 220px;
  }
  .preference-feedback p {
    margin: 0;
  }
  @media (max-width: 640px) {
    .preference-fields {
      grid-template-columns: minmax(0, 1fr);
      gap: 0;
    }
  }
</style>
