<script lang="ts">
  import { onDestroy, tick, untrack } from 'svelte';
  import { Autosaver } from '../autosave.svelte';
  import {
    getPersonalization,
    savePersonalization,
    getAutomaticLearning,
    forgetAutomaticLearning,
    resumeAutomaticSpellings,
    suspendAutomaticCandidate,
    approveAutomaticCandidate,
    chooseProjectApplication,
    chooseProjectFolder,
    type ProjectRule,
    type AutomaticLearningState,
    type PersonalizationProfile,
    type OutputIntent,
  } from '../api/personalization';
  import type { Language } from '../config';
  import Select from './Select.svelte';
  import Workspaces from './Workspaces.svelte';
  let {
    desktop,
    disabled = false,
    visible = true,
    onSaveActivity,
  }: {
    desktop: boolean;
    disabled?: boolean;
    visible?: boolean;
    onSaveActivity?: (saving: boolean, savedAt: number | null) => void;
  } = $props();
  let profile = $state<PersonalizationProfile | null>(null);
  let savedProfile = $state<PersonalizationProfile | null>(null);
  let learning = $state<AutomaticLearningState | null>(null);
  let pending = $state<'loading' | 'saving' | 'choosing' | null>(null);
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
  const projectModes = [
    { value: 'manual' as const, label: 'Choose manually' },
    { value: 'automatic' as const, label: 'Use application rules' },
  ];
  async function addProjectRule() {
    if (!profile || controlsDisabled || profile.projectRules.length >= 32) return;
    pending = 'choosing';
    error = '';
    let addedId = '';
    try {
      const application = await chooseProjectApplication();
      if (application) {
        const id = crypto.randomUUID();
        profile.projectRules.push({
          id,
          applicationId: application.id,
          applicationName: application.name,
          project: '',
          folder: null,
          enabled: true,
        });
        addedId = id;
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      pending = null;
    }
    await tick();
    if (addedId && visible) document.getElementById(`project-${addedId}`)?.focus();
  }
  async function setProjectFolder(rule: ProjectRule) {
    if (controlsDisabled) return;
    pending = 'choosing';
    error = '';
    try {
      const path = await chooseProjectFolder();
      if (path) rule.folder = path;
    } catch (cause) {
      error = String(cause);
    } finally {
      pending = null;
    }
  }
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
      // Removal is the only off-switch: everything listed is used.
      for (const word of profile.vocabulary) word.enabled = true;
      // Words you added are always confirmed, even before the next save migrates them.
      for (const word of profile.vocabulary)
        if (word.automaticSource === null) word.confirmed = true;
      savedProfile = $state.snapshot(profile);
      learning = await getAutomaticLearning();
    } catch (e) {
      error = String(e);
    } finally {
      pending = null;
    }
  }
  // Debounced autosave: every profile edit schedules a write 800ms out.
  // Explicit actions flush pending edits first so the revision guard sees
  // stored state; a failed flush aborts the action and the pill keeps
  // retrying silently.
  const panelSaver = new Autosaver(saveNow, 800);
  onDestroy(() => panelSaver.dispose());
  $effect(() => {
    const snapshot = profile === null ? null : JSON.stringify(profile);
    const saved = savedProfile === null ? null : JSON.stringify(savedProfile);
    const writable =
      snapshot !== null && !disabled && visible && pending !== 'loading' && pending !== 'choosing';
    if (writable && snapshot !== saved) panelSaver.notify();
    onSaveActivity?.(panelSaver.status === 'saving', panelSaver.savedAt);
  });
  async function saveNow(): Promise<boolean> {
    if (!profile || controlsDisabled) return false;
    if (JSON.stringify(profile) === JSON.stringify(savedProfile)) return true;
    pending = 'saving';
    try {
      profile = await savePersonalization($state.snapshot(profile));
      savedProfile = $state.snapshot(profile);
      learning = await getAutomaticLearning();
      panelSaver.noteSaved();
    } catch {
      return false;
    } finally {
      pending = null;
    }
    return true;
  }
  async function forgetLearning() {
    if (!profile || controlsDisabled) return;
    if (!(await panelSaver.flush())) return;
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
  async function resumeSpellings() {
    if (controlsDisabled) return;
    pending = 'saving';
    error = '';
    feedback = '';
    try {
      learning = await resumeAutomaticSpellings();
      feedback = 'Learned spellings resumed.';
    } catch (cause) {
      error = String(cause);
    } finally {
      pending = null;
    }
  }
  async function approveCandidate(id: string) {
    if (!profile || controlsDisabled) return;
    if (!(await panelSaver.flush())) return;
    pending = 'saving';
    error = '';
    feedback = '';
    try {
      profile = await approveAutomaticCandidate(id);
      savedProfile = $state.snapshot(profile);
      learning = await getAutomaticLearning();
      feedback = 'Confirmed. Vesper will use this spelling.';
    } catch (cause) {
      error = String(cause);
    } finally {
      pending = null;
    }
  }
  // The tick flips the confirmed flag on automatic entries; the debounced
  // autosave persists it. Manual entries are always confirmed.
  function toggleConfirmed(id: string) {
    if (!profile || controlsDisabled) return;
    const word = profile.vocabulary.find((entry) => entry.id === id);
    if (!word || word.automaticSource === null) return;
    word.confirmed = !word.confirmed;
  }
  async function dismissCandidate(id: string) {
    if (!profile || controlsDisabled) return;
    if (!(await panelSaver.flush())) return;
    pending = 'saving';
    error = '';
    feedback = '';
    try {
      await suspendAutomaticCandidate(id);
      profile = await getPersonalization();
      for (const word of profile.vocabulary) word.enabled = true;
      for (const word of profile.vocabulary)
        if (word.automaticSource === null) word.confirmed = true;
      savedProfile = $state.snapshot(profile);
      learning = await getAutomaticLearning();
      feedback = 'Removed. It will not be learned again.';
    } catch (cause) {
      error = String(cause);
    } finally {
      pending = null;
    }
    await tick();
    if (!error) document.getElementById('add-vocabulary')?.focus();
  }
  let vocabFilter = $state<'all' | 'mine' | 'learned'>('all');
  let newWord = $state('');
  let newFix = $state('');
  let addError = $state('');
  // Proposals not yet in the vocabulary render as Learning rows in the same
  // list. A listed word is never a proposal: everything in vocabulary is used.
  const learningRows = $derived(
    (learning?.candidates ?? [])
      .filter(
        (c) =>
          (c.state === 'observed' || c.state === 'provisional') &&
          !profile?.vocabulary.some((word) => word.phrase.toLowerCase() === c.phrase.toLowerCase()),
      )
      .slice()
      .reverse(),
  );
  const visibleWords = $derived(
    !profile
      ? []
      : vocabFilter === 'mine'
        ? profile.vocabulary.filter((word) => word.automaticSource === null)
        : vocabFilter === 'learned'
          ? profile.vocabulary.filter((word) => word.automaticSource !== null)
          : profile.vocabulary,
  );
  // Learning rows share the list under All and Learned, never under Yours.
  const showLearning = $derived(vocabFilter !== 'mine');
  async function addWord() {
    if (!profile || controlsDisabled || profile.vocabulary.length >= 128) return;
    const heard = newWord.trim();
    const intended = newFix.trim() || heard;
    addError = '';
    if (intended) {
      const dupe = profile.vocabulary.some(
        (word) => word.phrase.toLowerCase() === intended.toLowerCase(),
      );
      if (dupe) {
        addError = 'That word is already in your vocabulary.';
        return;
      }
    }
    const id = crypto.randomUUID();
    profile.vocabulary.unshift({
      id,
      phrase: intended,
      language: 'auto',
      context: '',
      enabled: true,
      confirmed: true,
      aliases: newFix.trim() && heard ? [heard] : [],
      project: profile.activeProject,
      pronunciation: false,
      sources: [],
      learnedAt: '',
      automaticSource: null,
      usageCount: 0,
    });
    newWord = '';
    newFix = '';
    await tick();
    document.getElementById(`word-${id}`)?.focus();
  }
  async function removeWord(id: string) {
    if (!profile || controlsDisabled) return;
    profile.vocabulary = profile.vocabulary.filter((word) => word.id !== id);
    await tick();
    document.getElementById('add-vocabulary')?.focus();
  }
  // History may have remembered new corrections. Refresh clean forms when the
  // panel is (re)shown, but keep unsaved edits and let the profile revision
  // guard resolve conflicts. Dirty flips from autosave must not reload: the
  // stored profile already matches the draft, and a reload would briefly
  // disable the form after every save.
  let lastShownKey = '';
  $effect(() => {
    const shownKey = `${desktop}/${visible}`;
    const reshown = untrack(() => {
      const changed = shownKey !== lastShownKey;
      lastShownKey = shownKey;
      return changed;
    });
    if (!desktop || !visible || !reshown) return;
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
      You can remove or forget learned words below.
    </p>
  </header>
  {#if !desktop}
    <p class="note">Personal preferences are available in the desktop app.</p>
  {:else}
    <form
      onsubmit={(event) => {
        event.preventDefault();
      }}
      aria-busy={pending !== null}
    >
      {#if profile}
        <fieldset disabled={controlsDisabled}>
          <legend class="visually-hidden">Personal preferences</legend>
          <div class="preference-fields">
            <div class="field">
              <label for="project-mode">Project selection</label>
              <Select
                id="project-mode"
                bind:value={profile.projectMode}
                options={projectModes}
                disabled={controlsDisabled}
              />
            </div>
            <div class="field">
              <label for="personal-project"
                >{profile.projectMode === 'automatic' ? 'Project override' : 'Current project'}
                <span class="optional">(optional)</span></label
              >
              <input
                id="personal-project"
                bind:value={profile.activeProject}
                maxlength="64"
                placeholder="For example, work or a project name"
                aria-describedby="personal-project-help"
              />
              <p id="personal-project-help" class="note">
                {#if profile.projectMode === 'automatic'}
                  Leave blank to use the app where you start shortcut dictation. Enter a name to
                  override the rules.
                {:else}
                  Enter a project name before recording. Leave blank to use shared vocabulary and
                  examples.
                {/if}
                Imports and recordings started here use this name. Each recording keeps its original project.
              </p>
            </div>
            {#if profile.projectMode === 'automatic'}
              <div class="field project-rules">
                <h3>Application rules</h3>
                <p class="note">
                  Choose an app and name its project. TextEdit can also match a folder. Missing or
                  conflicting context uses shared vocabulary.
                </p>
                {#each profile.projectRules as rule (rule.id)}
                  <div class="project-rule">
                    <label class="checkbox-field"
                      ><input
                        type="checkbox"
                        bind:checked={rule.enabled}
                      />{rule.applicationName}</label
                    >
                    <label for={`project-${rule.id}`}>Project for {rule.applicationName}</label>
                    <input
                      id={`project-${rule.id}`}
                      bind:value={rule.project}
                      maxlength="64"
                      placeholder="Project name"
                      required
                    />
                    {#if rule.folder}<p class="note project-folder">Folder: {rule.folder}</p>{/if}
                    <div class="actions">
                      {#if rule.applicationId === 'com.apple.TextEdit'}
                        <button
                          type="button"
                          class="secondary"
                          onclick={() => setProjectFolder(rule)}
                          >{rule.folder ? 'Change folder' : 'Choose folder'}</button
                        >
                      {/if}
                      {#if rule.folder}<button
                          type="button"
                          class="secondary"
                          onclick={() => {
                            rule.folder = null;
                          }}>Use entire app</button
                        >{/if}
                      <button
                        type="button"
                        class="secondary"
                        aria-label={`Remove ${rule.applicationName} project rule`}
                        onclick={() => {
                          if (profile)
                            profile.projectRules = profile.projectRules.filter(
                              (item) => item.id !== rule.id,
                            );
                        }}>Remove rule</button
                      >
                    </div>
                  </div>
                {/each}
                <button
                  type="button"
                  class="secondary"
                  disabled={controlsDisabled || profile.projectRules.length >= 32}
                  onclick={addProjectRule}>Add application rule</button
                >
              </div>
            {/if}
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
                off. Works with every supported speech model.
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
                  aria-describedby="personal-speech-hints-help"
                /><span>Request extra speech hints when supported</span></label
              >
              <p id="personal-speech-hints-help" class="note checkbox-help">
                Optional recognition help where available; may slow processing or affect other
                words. Saved corrections and spelling learning work with every speech model without
                this option. Your preference stays saved when you switch models.
              </p>
            </div>
          </div>
          <Workspaces bind:profile disabled={controlsDisabled} onprofilechange={load} />
          <div class="preference-group" id="vocabulary">
            <div class="group-heading">
              <h3 id="vocabulary">Vocabulary</h3>
              <span class="entry-count">{profile.vocabulary.length} / 128</span>
            </div>
            <p class="note">Everything here is used. Remove anything you do not want.</p>
            <div class="add-bar">
              <input
                id="new-word"
                bind:value={newWord}
                placeholder="Add a word, or what Vesper hears"
                aria-label="Word"
                maxlength="128"
              />
              <input
                id="new-fix"
                bind:value={newFix}
                placeholder="Should be (optional)"
                aria-label="Correct spelling"
                maxlength="128"
              />
              <button
                id="add-vocabulary"
                type="button"
                class="primary"
                onclick={() => {
                  void addWord();
                }}
                disabled={controlsDisabled || profile.vocabulary.length >= 128}>Add</button
              >
            </div>
            {#if addError}<p class="error" role="alert">{addError}</p>{/if}
            <div class="filter-pills" role="group" aria-label="Filter vocabulary">
              <button
                type="button"
                aria-pressed={vocabFilter === 'all'}
                onclick={() => (vocabFilter = 'all')}>All</button
              >
              <button
                type="button"
                aria-pressed={vocabFilter === 'mine'}
                onclick={() => (vocabFilter = 'mine')}>Yours</button
              >
              <button
                type="button"
                aria-pressed={vocabFilter === 'learned'}
                onclick={() => (vocabFilter = 'learned')}
                >Learned{#if learningRows.length}<span class="count-badge"
                    >{learningRows.length}</span
                  >{/if}</button
              >
            </div>
            {#if showLearning && learningRows.length}
              <ul class="term-list" aria-label="Learning words">
                {#each learningRows as candidate (candidate.id)}
                  <li class="word-entry learning">
                    <div class="entry-heading">
                      <button
                        type="button"
                        class="icon-btn tick"
                        aria-label={`Confirm ${candidate.phrase}`}
                        title="Confirm this spelling"
                        disabled={controlsDisabled}
                        onclick={() => {
                          void approveCandidate(candidate.id);
                        }}
                        ><svg
                          viewBox="0 0 16 16"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          aria-hidden="true"><path d="M3 8.5l3.5 3.5L13 4.5" /></svg
                        ></button
                      >
                      <span class="term-static"
                        >{#if candidate.alias}<span class="pair-from">{candidate.alias}</span><span
                            class="pair-arrow"
                            aria-hidden="true">→</span
                          >{/if}{candidate.phrase}</span
                      >
                      <span class="row-badge">Learning</span>
                      <button
                        type="button"
                        class="icon-btn"
                        aria-label={`Remove ${candidate.phrase}`}
                        title="Remove; it will not be learned again"
                        disabled={controlsDisabled}
                        onclick={() => {
                          void dismissCandidate(candidate.id);
                        }}
                        ><svg
                          viewBox="0 0 16 16"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg
                        ></button
                      >
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}
            {#if !visibleWords.length && !(showLearning && learningRows.length)}<p class="note">
                {vocabFilter === 'all'
                  ? 'Add names or terms you want Vesper to get right.'
                  : vocabFilter === 'mine'
                    ? 'Words you add yourself will appear here.'
                    : 'Words Vesper picks up on its own will appear here.'}
              </p>{/if}
            <ul class="term-list">
              {#each visibleWords as word (word.id)}
                <li class="word-entry">
                  <div class="entry-heading">
                    {#if word.automaticSource === null}
                      <span
                        class="tick-static"
                        title="Words you add are always confirmed"
                        aria-label={`${word.phrase || 'Word'} confirmed`}
                        role="img"
                        ><svg
                          viewBox="0 0 16 16"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          aria-hidden="true"><path d="M3 8.5l3.5 3.5L13 4.5" /></svg
                        ></span
                      >
                    {:else}
                      <button
                        type="button"
                        class="icon-btn tick"
                        aria-pressed={word.confirmed}
                        aria-label={word.confirmed
                          ? `Unconfirm ${word.phrase || 'word'}`
                          : `Confirm ${word.phrase || 'word'}`}
                        title={word.confirmed
                          ? 'Confirmed: always trusted'
                          : 'Confirm this spelling'}
                        disabled={controlsDisabled}
                        onclick={() => toggleConfirmed(word.id)}
                        ><svg
                          viewBox="0 0 16 16"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          aria-hidden="true"><path d="M3 8.5l3.5 3.5L13 4.5" /></svg
                        ></button
                      >
                    {/if}
                    {#if word.aliases[0]}<span class="pair-from">{word.aliases[0]}</span><span
                        class="pair-arrow"
                        aria-hidden="true">→</span
                      >{/if}
                    <input
                      id={`word-${word.id}`}
                      class="term-input"
                      aria-label="Preferred spelling"
                      bind:value={word.phrase}
                      oninput={() => {
                        word.sources = [];
                        word.automaticSource = null;
                      }}
                      placeholder="Project or person’s name"
                      maxlength="128"
                      required
                    />
                    <span class="lang-select"
                      ><Select
                        id={`word-language-${word.id}`}
                        bind:value={word.language}
                        options={languageOptions}
                        disabled={controlsDisabled}
                        onChange={() => {
                          word.sources = [];
                          word.automaticSource = null;
                        }}
                      /></span
                    >
                    <button
                      type="button"
                      class="icon-btn"
                      aria-label={`Remove ${word.phrase || 'word'}`}
                      onclick={() => {
                        void removeWord(word.id);
                      }}
                      ><svg
                        viewBox="0 0 16 16"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.8"
                        aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg
                      ></button
                    >
                  </div>
                </li>
              {/each}
            </ul>
            <details>
              <summary>How does this work?</summary>
              <p class="note">
                Everything here is used. Tick a word to confirm it: confirmed words are always
                trusted. Unticked automatic words are used when they match what you say. Remove
                anything you do not want; removed words never come back.
              </p>
            </details>
            {#if learning?.breakerTrippedMs}
              <div class="learn-paused" role="status" id="learned-spellings-paused">
                <p class="note">
                  Learned spellings are paused because you undid two of them recently. Words you
                  added or ticked still apply.
                </p>
                <button
                  type="button"
                  class="quiet"
                  disabled={controlsDisabled}
                  onclick={() => {
                    void resumeSpellings();
                  }}>Resume learned spellings</button
                >
              </div>
            {/if}
            <div class="learn-foot">
              <label class="checkbox-field" for="learn-from-edits"
                ><input
                  id="learn-from-edits"
                  type="checkbox"
                  bind:checked={profile.automaticLearning}
                /><span>Learn from my edits</span></label
              >
              <button
                type="button"
                class="quiet"
                disabled={controlsDisabled}
                onclick={forgetLearning}>Forget automatic learning</button
              >
            </div>
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
        </fieldset>
      {/if}
      <div class="preferences-footer" hidden={!error && pending === null && !feedback}>
        <div class="preference-feedback" aria-live="polite">
          {#if error}<p class="error" role="alert">{error}</p>
          {:else if pending === 'loading'}<p class="note">Loading personal preferences…</p>
          {:else if pending === 'saving'}<p class="note">Saving personal preferences…</p>
          {:else if feedback}<p class="success">{feedback}</p>{/if}
        </div>
        <div class="form-actions">
          {#if profile}
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
  .project-rules {
    grid-column: 1 / -1;
  }
  .project-rule {
    display: grid;
    gap: 0.5rem;
    padding-block: 0.75rem;
  }
  .project-folder {
    overflow-wrap: anywhere;
  }
  .personalization {
    margin-top: var(--space-6);
  }
  .personalization-heading {
    margin-bottom: var(--space-4);
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
  .example-entry {
    padding: var(--space-4);
    margin-bottom: var(--space-4);
    background: var(--color-bg);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
  }
  .add-bar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
  }
  .add-bar input {
    flex: 1 1 140px;
    min-width: 0;
  }
  .filter-pills {
    display: flex;
    gap: var(--space-1);
    width: max-content;
    max-width: 100%;
    overflow-x: auto;
    margin-bottom: var(--space-2);
    padding: 3px;
    background: var(--color-bg);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-pill);
  }
  @media (max-width: 480px) {
    /* Narrow screens cannot fit all four pills in one scroll strip without
     * clipping the last one, so let them wrap instead. */
    .filter-pills {
      width: 100%;
      flex-wrap: wrap;
      overflow-x: visible;
    }
  }
  .filter-pills button {
    appearance: none;
    border: 1px solid transparent;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--color-muted);
    font: inherit;
    font-size: 12px;
    padding: var(--space-1) var(--space-3);
    cursor: pointer;
  }
  .filter-pills button:hover {
    color: var(--color-ink);
  }
  .filter-pills button[aria-pressed='true'] {
    background: var(--color-raised);
    color: var(--color-ink);
  }
  .filter-pills button:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .count-badge {
    display: inline-block;
    min-width: 18px;
    margin-left: var(--space-1);
    padding: 0 var(--space-1);
    border-radius: var(--radius-pill);
    background: var(--color-tint);
    color: var(--color-accent);
    font-size: 11px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .icon-btn.tick[aria-pressed='true'] {
    color: var(--color-accent);
  }
  .tick-static {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    color: var(--color-muted);
  }
  .tick-static svg {
    width: 14px;
    height: 14px;
  }
  .row-badge {
    flex: none;
    color: var(--color-muted);
    font-size: 11px;
  }
  .term-list {
    list-style: none;
    margin: 0 0 var(--space-3);
    padding: 0;
  }
  .word-entry {
    padding: var(--space-2) 0;
    border-top: 1px solid var(--color-line);
  }
  .word-entry .entry-heading {
    margin-bottom: 0;
    flex-wrap: nowrap;
  }
  .term-input {
    flex: 1 1 auto;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-weight: 600;
    padding: 0;
  }
  .term-input:focus {
    outline: none;
    border-bottom: 1px solid var(--color-accent);
  }
  .term-static {
    flex: 1 1 auto;
    min-width: 0;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .pair-from {
    color: var(--color-muted);
    font-weight: 400;
  }
  .pair-arrow {
    color: var(--color-muted);
    margin: 0 var(--space-2);
  }
  .lang-select {
    flex: none;
  }
  .icon-btn {
    appearance: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
  }
  .icon-btn:hover:enabled {
    background: var(--color-raised);
    color: var(--color-ink);
  }
  .icon-btn:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .icon-btn svg {
    width: 14px;
    height: 14px;
  }
  .icon-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .word-entry.learning .term-static {
    color: var(--color-muted);
    font-weight: 400;
  }
  .learn-paused {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }
  .learn-foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-line);
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
