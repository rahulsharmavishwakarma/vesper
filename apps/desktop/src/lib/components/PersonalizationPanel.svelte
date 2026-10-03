<script lang="ts">
  import { tick, untrack } from 'svelte';
  import {
    getPersonalization,
    savePersonalization,
    getAutomaticLearning,
    forgetAutomaticLearning,
    chooseProjectApplication,
    chooseProjectFolder,
    type ProjectRule,
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
  }: {
    desktop: boolean;
    disabled?: boolean;
    visible?: boolean;
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
      You can remove or forget learned words below.
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
                  Enter a project name and save before recording. Leave blank to use shared
                  vocabulary and examples.
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
          <div class="preference-group">
            <div class="group-heading">
              <h3 id="vocabulary">Vocabulary</h3>
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
              <div class="group-heading">
                <h3>Automatic learning evidence</h3>
                <span class="entry-count"
                  >{learning.candidates.filter((c) => c.state === 'active').length} active · {learning.candidates.filter(
                    (c) => c.state === 'provisional',
                  ).length} awaiting · {learning.candidates.filter((c) => c.state === 'suspended')
                    .length} paused · {learning.candidates.filter((c) => c.state === 'observed')
                    .length} observed</span
                >
              </div>
              <p class="note">
                Vesper learns spellings from edits you save in Dictate. Dictation and copying alone
                do not confirm a spelling, and nothing here changes your vocabulary by itself.
              </p>
              {#if !learning.events.some((event) => event.kind === 'user_edit')}
                <p class="note">
                  No corrections recorded yet — edit Your text in Dictate and save to teach your
                  first spelling.
                </p>
              {/if}
              <p class="note">
                Ordinary spellings need corrections in two independent recordings and matching
                context. Select a recording language in Settings: automatic language mode keeps
                learned spellings provisional. History reviews require your confirmation; background
                intent and policy evaluation does not yet change your preferences.
              </p>
              {#if learning.candidates.some((c) => c.state === 'observed')}
                <div class="sub-heading">
                  <h4>Observed terms</h4>
                  <span class="entry-count">Never activate correction rules</span>
                </div>
                {#each learning.candidates
                  .filter((c) => c.state === 'observed')
                  .slice()
                  .reverse()
                  .slice(0, 8) as term (term.id)}
                  <div class="word-entry">
                    <div class="entry-heading">
                      <h4>
                        {term.phrase}
                        <span class="entry-count">observed {term.observedCount}×</span>
                      </h4>
                      <a class="term-manage" href="#vocabulary">Manage in Vocabulary</a>
                    </div>
                    <p class="note">
                      {term.project || term.context || 'Shared vocabulary'} · {term.language ===
                      'auto'
                        ? 'Language not established'
                        : term.language} · {term.reason}
                    </p>
                  </div>
                {/each}
              {/if}
              <details>
                <summary>Inspect recent candidates</summary>
                <p class="note">
                  {learning.events.filter((e) => e.retry === 'possible_repair').length} possible repair
                  signals. These do not create replacement rules. Usage counts do not prove a spelling
                  is correct.
                </p>
                {#each learning.candidates
                  .slice()
                  .reverse()
                  .slice(0, 32) as candidate (candidate.id)}
                  <p>
                    <strong>{candidate.phrase}</strong>{candidate.alias
                      ? ` ← ${candidate.alias}`
                      : ''} · {candidate.state} · {candidate.support.length} independent edit sources<br
                    /><span class="note"
                      >{candidate.project || candidate.context || 'Shared vocabulary'} · {candidate.language ===
                      'auto'
                        ? 'Language not established'
                        : candidate.language} · {candidate.reason}</span
                    >
                  </p>
                {/each}
                <p class="note">
                  Shows up to 32 recent candidates. Active entries can be edited, paused or removed
                  in Vocabulary.
                </p>
              </details>
              <div class="danger-zone">
                <button type="button" disabled={controlsDisabled || dirty} onclick={forgetLearning}
                  >Forget automatic learning</button
                >
                <p class="note">
                  Removes automatic spellings, candidates and usage evidence. Saved words and
                  reviewed recordings remain.
                </p>
              </div>
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
  .sub-heading {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
    margin: var(--space-5) 0 var(--space-3);
  }
  .sub-heading h4 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
  }
  .term-manage {
    font-size: 12px;
    white-space: nowrap;
  }
  .word-entry .note {
    margin-bottom: 0;
  }
  .danger-zone {
    margin-top: var(--space-5);
    padding-top: var(--space-4);
    border-top: 1px dashed var(--color-line);
  }
  .danger-zone .note {
    margin-top: var(--space-2);
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
