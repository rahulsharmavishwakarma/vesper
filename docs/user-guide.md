# Using Vesper

Vesper is a desktop dictation app for Apple-silicon Macs. It transcribes
microphone recordings and audio files locally, with optional text cleanup,
recording history and personal vocabulary.

Start at the [product README](../README.md) for download status or follow the
[development guide](development.md) to run from source. The public DMG and Apple
Developer ID signing/notarization are not available yet. Once a release is
available, open `vesper-macos-arm64.dmg` and drag `Vesper.app` onto Applications in the installer window.
The GitHub repository is named `vesper`; a local checkout may still be called `transcribe`. Existing settings, models and history
are reused in place when no Vesper store exists; nothing is moved, merged or reset.
The new application identifier may require granting macOS microphone,
Accessibility or notification permissions again. Browser preview reads older
settings until you explicitly save/reset, then writes only the Vesper key. Use **Vesper** when referring to the
app in documentation and issues; [technical compatibility names](development.md#product-name-and-compatibility)
do not indicate a different application.

## First launch

A new installation opens with a short guided setup (issue #270):

1. **Welcome**, then **Get started**.
2. **Voice**: choose the language you speak from the full list, pick a
   microphone if you have more than one, allow the microphone when macOS asks,
   and say a few words until Vesper shows **Heard you**. If your language needs
   a different speech model, Vesper says which and selects it for the next
   step.
3. **Models**: Vesper shows this Mac (chip, memory, free space) and suggests
   the best speech model for your language plus the writing model behind
   **Tidy my text**. Click either model to see the others, best first for your
   language, with small rings for accuracy, speed and memory. The speech
   suggestion comes from Vesper's own tests on held-out recordings in English,
   French, German, Spanish and Hindi; models are tagged _Passed Vesper's test_,
   _Failed Vesper's test_ (with the reason) or _Not tested by Vesper_, where
   the model maker's published figure is shown instead. **Download and
   set up** starts the download in the background; progress stays at the
   bottom left while you carry on. A download that does not fit in the free
   space, or needs a connection you do not have, is blocked with the reason.
4. **Access**: turn on Accessibility so Vesper can type into other apps. If it
   will not turn on, **Restart Vesper** (setup comes back to this step) or
   **Keep text in Vesper**, where your words wait, ready to copy.
5. **Shortcut**: keep or change Hold to talk and Press to toggle.
6. **Your words**: choose **As I said it** or **Polished**, and add names and
   terms Vesper should spell your way. Vesper also learns from your
   corrections on this Mac.
7. **Try it**: once the models are ready, click **Record**, speak, and click
   **Stop and transcribe**. You can also hold **Record** or your shortcut while
   you speak. Your words appear live as you talk, then land in the practice
   box. **Finish** and
   **Start dictating** open Dictate.

**Back** (or a finished step in the sidebar) returns to an earlier step;
going back from Voice shows Welcome again.

Setup appears once per user. Finishing it is remembered on this Mac, so later
launches and app updates open Vesper normally, and people who already used
Vesper before this setup existed never see it. Quitting part way resumes at the
same step. Everything chosen here can be changed later in Settings and Models.

New dataset export folders use `vesper-export-<uuid>`. Existing exported folders
remain valid and are never renamed automatically; their data format is unchanged.

### Small windows

When the window is narrow, navigation moves to the top and spacing becomes
more compact. Both configured model names remain visible in Settings → Recording and
History. Settings section navigation moves above its content when space is tight.
Scroll vertically to reach the remaining controls. Resizing preserves unsaved preferences; an open dropdown closes and can
be reopened in its new position.

### Settings

Use the interior sidebar to switch between Recording, Writing, Shortcuts,
Appearance and About. Changing sections keeps unsaved preferences intact.
Preferences save automatically about a second after each change, across
sections, with no Save or Discard action. A small pill at the bottom of the
sidebar slides in showing Saving…, flips to a tick with the save time, then
slides away on its own. Invalid choices are never written: they stay on
screen with guidance until fixed. Downloading a model first flushes any
pending edit so the saved selection is what downloads. Personal vocabulary
in Writing saves the same way.

Recording contains language, notifications, saved recordings and model names.
Its language choices follow both the speech model and the enabled writer. Change
polishing in Writing, then return to Recording to choose from the updated list.
Less-used storage options and Restore defaults are under **Advanced storage and
improvement**. Restore defaults saves the default application preferences; it
does not remove recordings or vocabulary.

In **Appearance**, choose Light, Dark or System. The color theme applies immediately
throughout the app and is remembered locally; System follows device appearance.
Light uses white cards on a white canvas with a warm off-white sidebar, near-black text
and ember highlights for actions and selected controls.
Dark is the default. Clearing browser/webview storage resets this visual preference.
Reduce Motion follows your system setting. Floating capture options on this page
save automatically like everything else. Themes in a browser preview and the installed app are
stored separately.

### App information

Open **Settings → About** to see app information in the content panel.
Choose **About Vesper…** in the native menu to open the same information in a sheet.
The Ember logo pairs the ember bird with a Bricolage wordmark in the sidebar and
About sheet; the app icon still uses the Songbird bird alone. The bird's call marks breathe with
an idle motion and tilt on hover; opening About replays the motion. The bird body
and wordmark stay still, and Reduce Motion disables that decoration.
It shows the compiled application version and one update control (see below);
it does not show a build type, platform or copy button, and includes no
recordings, transcripts or local paths. Escape or Close dismisses the native About
sheet without changing settings. In Settings, choose another section to leave About.

### Application updates

Open **Settings → About** (or the About sheet): one update pill beside the
version reports the state and offers the next step. It checks automatically, and
Vesper also checks quietly at startup after the window is ready, staying silent
when you are current. The pill reads **Checking…** with a turning arrow, then
**Up to date** (hover it for **Check now**), **Install 0.2.0** when an update is
available (its release notes appear below), **Downloading** with a progress fill
and percentage, and **Restart to update** once the verified package is ready.
Installation waits while you are recording, transcribing, cleaning up writing or
downloading a model, and it never force-restarts; the reason appears below the
pill. If a check, download or install fails, the pill shows **Couldn't check**,
**Download failed**, **Install failed** or **Restart failed**; hover it for
**Retry**. A check gives up after 20 seconds without an answer, so a slow or
offline network shows **Couldn't check** rather than **Checking…** indefinitely.
In a build without an enrolled updater it reads **Updates
unavailable** and the switches below are hidden. Model downloads continue to be
managed separately in **Models**.

Two switches under the version control the automation. **Check for updates
automatically** (on by default) makes the startup check and the check when About
first opens; turn it off and the pill offers **Check for updates** instead.
**Download and install automatically** (off by default, and unavailable while
checking is off) downloads and installs a release as soon as one is found, using
the same signature check and the same wait for active work; it never restarts
for you, so the pill ends at **Restart to update**. If work is in progress the
install waits and is retried at the next check. A third switch,
**Include beta and pre-release builds** (off by default), decides which releases
are offered. Off, only the latest stable release is offered. On, Vesper also looks
at release candidates and betas and offers whichever is the newest build that is
newer than the one you run, so a tester moves to the stable release once it
overtakes the last candidate. Pre-release builds may be less stable. Turning the
switch changes the offer at once: a candidate that was on offer disappears when you
turn it off, and a new check runs when automatic checks are on. If the pre-release
feed cannot be reached, you still get the stable answer. Nightly builds do not show
this switch; they follow their own feed. All three choices are remembered on
this device.

Every update package is signature-checked against Vesper's enrolled update
key before it installs; a package with a missing, corrupt, or wrong-key
signature is refused and the working installation is left untouched. The app
follows the stable feed only. If the update key ever changes, one transition
release accepts the new key first, so no working install is stranded. Update
signing (which proves a package came from Vesper) is separate from Apple
notarization (which proves macOS trusts it to run silently): unsigned builds
still verify and install through the app, but macOS may ask for confirmation
on first launch.

### Initial setup

1. Open **Models** and select speech and writing models. The writing model is
   used only when text cleanup is enabled. You do not choose its precision:
   Vesper downloads and uses the best qualified file automatically (currently
   the compact Q4 build). Each picker row shows the model's
   supported languages; a row with more than six languages shows a count that
   expands to the full list, and the writing model shows Many languages
   because it publishes no per-language list. The Needs row at the top of an
   open picker narrows both lists to models covering English, French or Hindi.
2. Download the selected files. Downloading first flushes any pending selection
   and saves it, then downloads it; changing a selection alone only saves it
   and does not download speech or writing
   weights. Supporting audio packages download automatically when those models are ready.
3. Open **Dictate**, select your microphone and choose a language supported by
   the speech model and the enabled writing model. Allow microphone access when macOS asks.
4. Wait until the app is ready before recording. The first recording may take
   longer while a model loads.

Model downloads need internet access and additional disk space. Files are
verified before use. Manage installed models and device
acceleration in **Models**. See [model choices](model-presets.md) for details.

Vesper automatically reuses verified model files already in your Hugging Face
cache, so matching files do not download again. The exact model version and
format must match; a CPU export and an Apple GPU export can need different files.
New Hugging Face downloads also use that shared cache. Files are shared where
possible; a model folder on another drive may need a local copy.

Removing a model removes it from Vesper and keeps the shared HF cache available
for other applications and later reinstalls. The displayed model size is its
logical size, not necessarily additional disk space. Existing downloads stay
where they are. See [cache location and behavior](internals.md#hugging-face-cache-boundary).

The language menu follows the selected speech model and processing device. With
polishing enabled, it also limits choices to the writer's declared languages.
Changing models does not silently change your language: resolve the inline
compatibility message before saving, or turn polishing off to use the speech
model's choices. **Detect automatically** identifies speech language; it does not
guarantee mixed-language accuracy. Custom writing presets require an explicit
language. An older writing profile with no language information remains readable;
reselect its preset in **Models**, choose the built-in writer, or turn polishing
off. Until repaired, writing is skipped with a warning and the transcript remains
available. See the [speech and writing language table](language-support-and-recommendations.md#writing-language-compatibility).

Each model card shows its selected model and download status. **Local engine**
above it shows readiness, CPU, memory and operation timings. Models are checked
automatically when loaded; the separate **Selected models** verification card is
not needed. If model files are missing or damaged, use **Download** or **Verify
files** on the affected model. Verification keeps valid files and downloads
replacements for missing or damaged files. Repairs need internet only when a
verified replacement is not already cached. They start from this action, not
automatically during transcription. Verifying one model leaves the other
models' status as it was; only the model being verified shows its progress.

While something runs (a model download, verify or removal, a recording,
transcription or polishing), the bottom of the left sidebar names it and,
for downloads, shows a progress bar with the percentage. It stays visible on
every page; click it to open the page where you can follow or cancel it. In
Settings, where the sidebar shrinks to icons, it becomes a dot over a short
bar and its text is in the tooltip.

Vesper picks how a speech model runs; there is no engine setting. On Apple
Silicon, Parakeet v3 in English runs on the Apple Neural Engine (a 640 MB
download), leaving the GPU to the writing model. Its other languages, and Macs
where the Neural Engine path cannot run, use one 714 MB file that runs on CPU
and on every supported GPU. After the first set installs, Vesper downloads the
other one quietly in the background, so changing the language needs no further
download; dictation keeps working meanwhile and a short status line shows its
progress. The first Neural Engine use after the download, and once after each
macOS update, takes a few extra seconds to prepare. See
[Parakeet on the Neural Engine](offline-asr.md#parakeet-on-the-neural-engine)
and [Parakeet unified GGUF engine](offline-asr.md#parakeet-unified-gguf-engine).

**Automatic** uses a compatible detected accelerator where the selected runtime
supports it. LFM Audio and the writing model share native GPU selection; on Apple
Silicon this can use Metal. If GPU initialization fails in Automatic mode, the
runtime retries on CPU and records the reason. An explicitly selected GPU fails
when unavailable instead of silently switching devices. The first GPU request
can take longer while native kernels initialize.

GPU support is specific to the model and runtime. On Apple Silicon, Automatic chooses the compatible GPU files for English Parakeet and Qwen ASR when this app includes the verified runtime and detects a usable GPU. Save your selection and use Download; no backend choice is required. Cohere and Whisper Turbo retain their CPU speech paths. Granite Speech 5.0 TurboCTC, Phonon-2 and Parakeet Redux run only on the GPU and never retry on CPU. The writing model makes its own accelerator selection. Whisper models are offered as optional picker entries by owner decision despite failed noise/repetition gates; see the [speech and writing language table](language-support-and-recommendations.md#writing-language-compatibility).
See [performance evidence](inference-performance.md)
for tested combinations and remaining limits.

New installations use Cohere Transcribe with optional Qwen writing cleanup and
English selected. The current Cohere adapter supports 14 explicit languages for
completed recordings (see the language guide); it has no automatic language
detection or Hindi, and its live words are buffered drafts. Other models have different capabilities. Use the
[language compatibility guide](language-support-and-recommendations.md) when
changing models rather than assuming every model supports every mode.

Model defaults are reviewed against measured accuracy, failures and performance
for the intended use. A newer release alone is not a reason to replace a model.
The current selection remains provisional; see the [benchmark selection policy](model-presets.md#choosing-the-default-from-benchmarks).

## Record or import audio

In **Dictate**, choose **Record**, wait for **Listening**, and speak. Choose
**Stop and transcribe** when finished. To transcribe an existing recording, use
the audio-file action instead. Review the result before using it.

After recognition finishes, **Recognized text** can appear while the app is
still polishing or finishing the recording. This read-only preview is the
original recognized text, before writing cleanup. It works for microphone
recordings and imported files; short jobs may finish before it appears.
Wait for **Your text** to edit or use **Copy text**. Cancellation hides the
preview while the current operation stops; any completed raw result remains
available afterward. Automatic insertion still waits for the finished result.

The waveform follows microphone level and estimated voice pitch. Quiet input
produces smaller strokes; unvoiced sounds still register without forcing the
spacing to jump. It is a recording indicator, not a speech/noise detector: fans,
music and other audible sounds can also move it.

### Live words while speaking

Current speech models can display draft words while you speak in Dictate and the
floating recorder. Wait for **Listening** before speaking. Loading a cold model
can delay readiness; speech before Listening is not captured.

Drafts can change as more speech arrives. Some models update more frequently
than others, especially on slower devices or under memory pressure. Very short
recordings may finish before the first draft. After **Stop**, most models perform
a final pass over the full recording, so the finished text can differ from the
live draft. The last draft stays on screen, dimmed and labelled **Finalizing**,
until the finished text replaces it. A moving waveform indicates microphone activity, not completed
recognition. See the [per-model behavior](model-presets.md#live-transcription-support).

Writing cleanup and the final paste into another application happen after the
recording finishes. Live drafts stay inside Vesper and are not used as learning
labels. If live updates cannot keep up, Vesper retains the recording for final
transcription. If a saved model produces no drafts on a longer recording, check
that Listening appeared and the microphone shows an input signal, and report any
runtime warning with the selected model, language and processing device.

### Recording length and cleanup

There is no general application recording-time quota. Available memory, model
limits and recognition quality still affect longer recordings. Supported
long-audio adapters process recordings in internal segments; this does not
make every model suitable for every duration.

Optional cleanup can improve punctuation and formatting. The original
transcript remains available for comparison, and failed or rejected cleanup
retains the raw text. Context-correction options can change wording and need
review. Edit **Your text** before copying when necessary. **Copy text** waits for
your edit to save and stays busy until copying finishes. If saving fails, your
edit remains visible; use **Retry saving edit**, then copy again.

With default Qwen formatting-only cleanup, the app preserves dictated
contractions and can automatically retry a failed short cleanup once. A retry
can add processing time. If the result still fails the checks, the original
transcript remains available. This recovery needs no setting or per-word setup.
Polishing also stops if a pass takes far longer than normal (5 seconds plus a
little more for long transcripts, about ten times the usual time); the original
transcript is kept and a message explains why.

### Experimental audio processing

Supporting audio models are managed in the background. There is no Audio
processing card or supporting-model status list in Settings.
Only the speech (ASR) and writing models have model selectors. Noise reduction
uses **DPDFNet8**, and **Silero v6.2** adds speech timing annotations without
removing recorded audio. Automatic volume adjustment is disabled. Older audio
preferences use this managed setup when loaded; your speech, writing, language
and storage choices are preserved. Existing DPDFNet4 files remain on disk but
are no longer selected by the desktop app.

Once your selected speech and enabled writing models are ready, the desktop app
downloads missing, supported audio packages automatically, one at a time. It waits
while you are recording, transcribing, downloading another model or editing settings.
Download progress and **Cancel download** are visible even outside Models. Setup
uses the existing installation operation, so recording and transcription wait
during an active download; cancellation lets you continue with original-audio
fallback. Only choices that conflict with a download are locked while it runs:
the model being downloaded (while the speech model downloads you can still
change the writing model, and the reverse), the processing
device, removing models and Restore defaults, each with a note saying why. Everything else stays usable,
including History (browse, import, export, review), the language and microphone,
Appearance, Shortcuts, Writing preferences and other settings, which save as usual. Files are
verified against pinned hashes. Downloads require internet access; transcription
and audio processing run entirely on device after setup.

Supporting files are checked automatically once per app session.
Valid stored files are reused; missing or corrupt files are downloaded and repaired.
No audio-model selection, download, verification or retry action is needed.
After a failed or cancelled transfer, setup retries after 30 seconds, then
2 minutes, with later retries spaced 10 minutes apart; reconnecting to the internet
also resumes setup. It always waits for recording and settings edits to finish.
ASR and writing
weights still use **Save & download**.

Noise reduction applies to live transcription and completed recordings. During a
live recording it runs once, while you speak, and the final transcript reuses that
processed audio, so stopping does not wait for the whole recording to be processed
again. It can
reduce transcription accuracy, especially for quiet speech; the existing
[denoising comparisons](validation/denoising-followup-2026-09-19.md) remain mixed.
This fixed product choice is not a claim of improved accuracy for every ASR model
or accent. Original recordings remain intact. Missing models or processing
failures use the original-audio fallback; failures are reported with a warning.

### Experimental speaker timelines

Speaker analysis runs automatically when its local model and runtime are
available; there is no enable switch or settings-save step. On first run the
app pre-checks the bundled speaker runtime and installs it into the model
directory, then its model downloads automatically once the selected
speech/writing models are ready. This background setup has no separate
Settings card.
This experimental integration requires Apple silicon; the runtime ships
inside the macOS app bundle (see
[experimental speaker runtime](development.md#experimental-speaker-runtime)).
It runs entirely on device and needs macOS 14 or later. If the bundled runtime
is missing or incompatible, transcription is unaffected and no timeline appears;
the speaker model's card in Models then says the bundled speaker runtime is
missing or incompatible. Downloading again does not fix this; reinstall Vesper.

For live recordings, speaker analysis runs after the text is delivered or
inserted, so it never delays your text; the timeline appears shortly afterwards.
Imported files are analyzed before the result is shown.

After ASR, **Speaker timeline** in the latest result and History shows estimated
start/end ranges for up to eight anonymous speakers. Overlapping ranges can
represent simultaneous speech. Speaker numbers apply only within that recording;
they do not identify you, assign individual transcript words, or improve spelling.
The view shows the first 200 ranges; dataset exports include all ranges.

Raw text is saved before speaker analysis. Missing or incomplete local components
and unsupported compute backends leave ordinary transcription unchanged. If
installed components fail verification or analysis fails, the transcript remains
available with a warning. Analysis adds processing time and memory; broader
speaker and language accuracy qualification remains incomplete.
With audio collection off, the latest timeline is available in memory without
creating a saved audio session. Stored timelines are unverified machine output
and never become approved training labels automatically.

## Quick capture

The desktop app must be running for its global shortcuts to work.

| Action          | Default Mac shortcut | Behavior                                 |
| --------------- | -------------------- | ---------------------------------------- |
| Hold to talk    | `⌥ Shift D`          | Hold while speaking; release to finish.  |
| Press to toggle | `⌘ Shift Space`      | Press once to start and again to finish. |

Both shortcuts can be changed in **Settings → Shortcuts**. Any combination
with a modifier works — letters, numbers, function keys and editing keys such
as Enter, Tab, Backspace or Delete. A bare key is not accepted, since it would
fire while you type. Existing
installations keep their saved shortcuts. Wait for the floating controls to
show **Listening** before speaking.

To insert a finished transcript into another app, select **Paste into the
original text field** in Settings → Shortcuts, enable the requested macOS
Accessibility access and save. If you just enabled it, quit and reopen Vesper
before checking again — macOS can keep reporting the old state until the app
restarts. The same restart applies after toggling the switch off and on again.
A freshly rebuilt or updated unsigned app is a new code identity to macOS, so
grant access again after replacing the build. Focus the intended field before
recording.
Hold release or Toggle Stop then inserts the finished transcript after
transcription and writing complete. Keep the original field and caret selected
until it finishes. **Keep in app** deliberately disables automatic paste;
main-window Record and imported files also keep their result in the app.

Check **Ready to paste** in Settings → Shortcuts before recording. On macOS
versions that request clipboard access, allow automatic clipboard access so
Vesper can preserve your clipboard without opening a prompt during delivery.
With automatic paste enabled, Vesper prepares supported Electron editors such
as VS Code when you switch to them. A freshly opened editor can take a moment
to expose its text field; if it is not ready when recording starts, that result
remains available through **Copy**. Changing to **Keep in app** stops this
preparation. Protected fields, Terminal, iTerm and Warp remain excluded.
When the focused control is a wrapper around the real field — common in
web-based editors such as Outlook's compose view — Vesper looks inside that
control for the field you are typing in, and waits briefly once if the editor
has not exposed its structure yet. In Excel and Numbers, selecting a cell is
enough: pasting fills exactly that selected cell, and moving to another cell
before delivery keeps the transcript available through **Copy** instead.
External-app compatibility is still being qualified.

All current speech adapters can show drafts in the floating recorder. Update
speed depends on the model and device; some models revise buffered speech in
batches. Select your model on **Models** and wait for **Listening** before speaking, particularly
after cold model startup. Live words stay in the recorder; the external field
receives one final paste after release.

You can change the capture design — **Capsule** (one slim pill), **Split**
(glass card with a separate action pill) or **Island** (a compact pill that
grows with the transcript) — move the floating controls, or hide them
while keeping shortcuts active. The overlay is frosted glass, so the
desktop tints through it while text stays readable. On macOS, click the
**bird logo** in the menu
bar for recording status and quick access to History, Models, Settings and Quit.
It shows only the still logo, without the Vesper wordmark or a background tile;
macOS adjusts its monochrome appearance. Hover for the Vesper name and status.
Closing the main window keeps the app running. Finish or cancel active work
before quitting.

## History and personal vocabulary

**History** contains saved recordings and transcripts. Search by text or
filename, filter by review status or date, and expand a recording to review or
copy its text. Export options distinguish a complete archive from reviewed
training examples; search filters do not limit an export. Every completed
recording appears here, including shortcut dictations finished in other apps:
the list refreshes itself when a recording completes, and returning to History
reloads it.

**Your recording rhythm** (top of History, last 7 or 30 days) shows **Time
saved**, **Time recorded**, **Words** and **Recordings**, a recordings-per-day
chart (pick a day to filter the list), active days, speaking pace and words per
recording. Time saved is how much longer typing the same words at 40 words a
minute would have taken than saying them; a recording without a known length
adds nothing.

Expanding a recording shows its transcript card, also used for the latest
result in Dictate. The card plays the saved audio (play/pause, seek bar,
1×–2× speed) and shows **Heard** beside **Polished · edit to teach**;
highlighted Heard words were changed or dropped by cleanup. Both panes grow
to fit long transcripts, so neither side clips mid-line. Answer **Is the
polished text right?** with a vote: a like approves the polished text (a
verified speech label only when it matches what was heard, otherwise a
reviewed correction) and teaches nothing by itself, while a dislike opens the
polished text for editing and saving the correction teaches Vesper from that
explicit edit. Approved cards show **Verified label** or **Learned from your
edit** with **Undo**, which returns the card to needs-review and clears the
vote without removing already taught vocabulary. **Details** lists the audio
file facts, the project, and per-recording speaker ranges when available.

Manage spellings, recurring corrections and writing preferences in
**Settings → Writing → Your vocabulary and writing**. The Vocabulary list holds
words you add, words Vesper learned from your edits, and proposals it is still
learning (badged Learning, not applied yet). Listed words are used until you
remove them. Tick a learned word to confirm it: confirmed words always apply,
while unticked ones apply when they match what you say. Ticking a proposal
adopts it as yours on the spot. Editing a row's spelling makes it yours too.
Removing a learned word or proposal blocks that spelling for good; removing a
word you added does not — a later edit can teach it again. Eligible edits to
**Your text** can contribute to local spelling memory for later recordings.
Raw predictions remain available, and remembered changes can be undone or
forgotten.

Correct spellings directly in **Your text**; there is no separate Remember step.
The app can learn eligible joined/split names, closely related spellings and
narrow acronym corrections, even when the edit includes minor formatting.
Ordinary terms need corrections from two independent recordings before they
are reused in matching context. Replaying the same recording does not count
twice. An everyday word that was really a name or acronym (for example *data* →
*Detta*, *rack* → *RAG*) is learned after three corrections in the same project.
It is then used only when the sentence shares at least two topic words with the
sentences you corrected, so "the training data" stays as it is. An everyday word
changed into another everyday word (*write* → *right*) is never applied
automatically. Add that spelling in Vocabulary if you want it. Sentence rewrites and changed decisions are not global dictionary rules,
and some valid corrections still fall outside the current learning rules.

If you restore an earlier spelling in the same draft, its withdrawn correction
no longer counts as supporting evidence. Correcting an automatically remembered
word to another eligible spelling removes the rejected word from your vocabulary,
and it will not be learned again. Fresh corrections
of the original recognition error can then teach its replacement; the app keeps
your current draft immediately while waiting for that support.

Personal vocabulary persists across model changes. A correction Vesper learned
from one speech model's mistakes waits while you use a different model and comes
back when you return; words you add or tick work with every model. If you undo
two different learned spellings within a week, Vesper pauses all unticked
learned spellings and shows **Resume learned spellings** under Vocabulary.
In a detected workspace, Vesper also fixes a misheard non-word that is one letter away from one of that workspace's names or terms, such as *vasper* → *Vesper*. It never changes ordinary English words. If you change a fixed word back, Vesper stops using that spelling for automatic fixes.
These local corrections do not train model weights or activate a new model. Read the
[personalization guide](personalization/README.md) for the exact learning,
review and undo behavior.

**Words Vesper found** (under Vocabulary, collapsed) lists spellings that
Vesper collected on its own. Sources:

- your Mac's **Text Replacements** and learned spelling words
- spelling lists, code names, package names and documentation headings in
  project folders you chose

A background check runs about once an hour while the Mac is idle and not too
warm. It skips Low Power Mode, and it reads project folders only on power. Each
recording uses only a few relevant words, preferring the current project's
words. Words found this way are never taken from what the speech model or the
writer produced.

- Remove a word with ×; it is not added again.
- **Forget** removes every word from one source and stops reading it until you
  choose **Resume**.

Vesper never asks you to choose folders to read. (The earlier **Learn words from
this folder…** button was removed on 28 October 2026; words read that way are
dropped on the next check.) See [term inventory](personalization/term-inventory.md).

**Project selection** can be manual or use application rules. In Vocabulary,
choose **Use application rules**, click **Add application rule**, choose an app,
and name its project, then save. Shortcut dictation freezes the receiving app's
project before recording. TextEdit rules can also match a selected folder; the
most specific matching folder wins. Missing document context or conflicting rules
uses shared vocabulary. Folder rules use the same document reader as automatic
workspaces, so they are no longer limited to TextEdit.
A nonempty **Project override** always wins. Main-window recordings and file imports
use the explicit project name. Results show the project and any fallback reason.

Give vocabulary and examples the same project name to limit their use; leave an
entry's project blank to share it. Reviewing an older recording keeps its original
project, including a blank project, rather than borrowing the current selection.
Changing rules or switching apps cannot move an in-flight recording's feedback
into another project. Vesper reads app identity and the focused window's document
location for folder rules and workspaces, never document contents to guess the project.

**Workspaces** (Writing → Workspaces) are off until you turn on **Detect
workspaces automatically**. Then they are detected when you dictate
with the shortcut and have not entered a project. Vesper notes the folder of the
file open in the focused window (for example a repository in VS Code or Terminal,
or a document folder) or, in a browser, only the website's name. A workspace
appears in the list after about five dictations on two different days; until
then dictation uses shared vocabulary. A visible workspace works like a project
named after its folder or website: words you teach while in it stay with it. In
the list you can **Rename** it (Enter saves, Escape cancels), **Merge…** it into
another workspace, or **Remove** it so it is never detected again. Renaming or
merging moves the vocabulary and examples saved under the old name.
A project you enter, and any application rule, always wins. Turn off **Detect
workspaces automatically** to stop detection; existing workspaces stay listed.
Detection relies on what each app exposes through Accessibility; apps that
expose nothing (for example Mail or Slack) use shared vocabulary. While
detection is on, Vesper counts, for each app you dictate into, how often it
found a file, folder or website and how long the read took. Only the app's
identifier and the counts are kept, never what was found, and turning detection
off deletes them.

**Apply my saved corrections** works with all supported speech models, including
when cleanup is off or the writer fails. **Request extra speech hints when supported**
is a persistent preference that can be saved with any model selected. The current
Nemotron adapter can use those hints; other adapters ignore the request and still
use saved corrections afterward. Learning evidence from independent recordings
can accumulate across model changes; replaying the same audio with another model
does not count as independent support. A manually added term
needs a **Heard as** alias to replace a recurring mistake after recognition;
listing its preferred spelling alone does not create that replacement.

If learning appears inactive, Vesper probably has not received the relevant
feedback. (The **Learning on this device** report in History was removed on
28 October 2026.) Edit
**Your text** and leave the field or copy to save it. Dictation frequency and Copy
do not verify spellings; corrections made in another app count only with **Learn
from fixes in other apps** on. Candidate messages
explain missing independent support, automatic-language mode, a full vocabulary
or an existing manually managed entry. Select a recording language and correct
new recordings to teach automatic aliases; older automatic-language observations
are not retrospectively assigned a language. History labels require your review;
background semantic/policy evaluation does not currently activate new preferences
or train models.

Ordinary recordings also collect conspicuous technical terms automatically as
observed candidates (up to 8 per result, bounded storage). They appear in the
**Vocabulary** list (Settings → Writing → Your vocabulary and writing) as
suggested words with their observation count, project and language, and never
activate correction rules until you confirm them with the tick. Use the
filter pills to show only suggested words.

Settings no longer offers the inactive **Improvement policy / Automatic
activation** selector. Its stored compatibility value does not enable a trainer,
automatic review or stronger RSI. Automatic spelling learning is separate.

Unticked automatically learned words are used after recognition only when the
transcript also matches context from a supporting edit. Older learned words may
need a fresh correction before they apply again. Ticking a word confirms it, so
it applies without the context match. This helps avoid unrelated replacements,
but can miss a valid correction when you phrase the context differently.
Manually saved vocabulary and optional recognition hints keep their existing
controls.

### What is checked or reviewed automatically?

| Step                        | Current behavior                                                                                                                                                                                                                         |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cleanup checks              | Runs number, negation and word-change checks against the transcript; rejected cleanup preserves raw text. These checks cannot certify that recognition or meaning is correct.                                                            |
| In-app spelling learning    | Eligible saved corrections contribute automatically to scoped vocabulary under the support and context rules above. No per-word approval is needed.                                                                                      |
| Background edit analysis    | After a saved in-app correction, an already-loaded supported writer can analyze intent and context while idle. Its findings are evaluation evidence; they do not change your preferences or approve labels. Recording cancels this work. |
| Learning-rule comparisons   | Explicit in-app edits trigger background comparisons of fixed learning rules. Results do not select a new active rule.                                                                                                                   |
| External-editor corrections | Not observed. Correct the draft inside Vesper; eligible in-app corrections teach vocabulary automatically.                                                                                                                               |
| History and model training  | History training labels require explicit review. Automatic label approval, candidate training and model promotion are not active.                                                                                                        |

Automatic checks therefore do not mark each recording as reviewed. Semantic
activation remains blocked on failed quality qualification; adaptive learning
rules and training have separate unfinished gates. See [semantic analysis](personalization/semantic-shadow.md),
[learning-rule evaluation](personalization/policy-experiment.md#persistent-application-shadow-evaluation)
and [training qualification](cleanup-adapter-qualification.md).

## Privacy and storage

Transcription and cleanup use installed local models. The app does not upload
audio or transcript text for inference. Downloading model/runtime files still
requires network access.

**Save recordings and transcripts** is enabled by default. Imported audio is
archived without changing the original file; microphone recordings are stored
locally with their transcript and processing information. Disable this setting
to prevent new session archiving. This does not delete existing History.

Audio retention and personal memory are separate. With recording collection
off, compact local correction evidence can still persist; turning off history
is not a switch that removes all local data. Manage memory in its own settings
and see [personalization storage and privacy](personalization/storage-and-privacy.md)
for retention, forgetting and export boundaries. Optional system notifications
do not include transcript text.

## Troubleshooting

If you run Vesper from source and `make dev` stops with a Swift cache I/O error,
see [missing Swift build/cache directories](development.md#missing-swift-buildcache-directories).
The build script can recover links to removed temporary build folders; this
does not remove your downloaded models or recordings.

| Problem                                            | First checks                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Waveform does not match the microphone             | Confirm the selected input device. After a native audio update, quit the running app and open the newly built version; a page reload does not update the audio code. Background sound can still register.                                                                                                                                                                                                                                                                    |
| Recording will not start                           | Confirm microphone permission, selected input device and model download status. Wait for Listening.                                                                                                                                                                                                                                                                                                                                                                          |
| Warning that microphone audio was skipped          | The Mac was too busy to keep up with the microphone; the warning shows how many seconds of audio were skipped. Vesper skipped that audio and kept recording. Close heavy applications and record the missing part again.                                                                                                                                                                                                                                                     |
| Multichannel audio interface sounds quiet          | Vesper uses only the channels that carry sound, so one microphone on a multichannel interface keeps its full level. Unused inputs on the interface do not need to be disabled.                                                                                                                                                                                                                                                                                               |
| A language or live captions are unavailable        | Check the selected speech model's [supported languages and modes](language-support-and-recommendations.md).                                                                                                                                                                                                                                                                                                                                                                  |
| A model download fails                             | Check the connection, free disk space and model-folder access, then retry ASR/writing downloads from Models. Supporting audio setup retries automatically.                                                                                                                                                                                                                                                                                                                   |
| A shortcut is unavailable                          | Resolve the conflict shown beside the shortcut setting or choose another combination.                                                                                                                                                                                                                                                                                                                                                                                        |
| Automatic paste fails                              | Choose **Paste into the original text field**, save, and check **Ready to paste**. Keep the original field focused. A newly opened Electron editor may need a moment before the first recording; use Copy for an unavailable destination. In a spreadsheet, select the destination cell first rather than opening it for editing. If a password prompt, Touch ID dialog or sudo session is open anywhere, dismiss it first — secure input blocks automatic paste everywhere. |
| Accessibility shows on but paste stays unavailable | Local builds are ad-hoc signed, so each rebuild or reinstall is a new identity to macOS and the old grant stops matching. Remove Vesper with the minus button, reinstall that exact build, re-grant access, restart the app, and retest.                                                                                                                                                                                                                                     |
| A recording is missing from History                | Check whether Save recordings and transcripts was enabled for that session.                                                                                                                                                                                                                                                                                                                                                                                                  |

If the problem persists, follow [Support](../SUPPORT.md). Include the application
version, macOS version, selected model and a synthetic reproduction. Do not
include private audio, transcripts or credentials.

## Background personalization analysis

When you save a correction inside Vesper, the app also attempts a small local analysis while idle, using the writing model only if it is already loaded. Recording interrupts this work. This experimental analysis helps evaluate intent and context; its guesses do not yet change your words or preferences. Automatic spelling learning continues as before.

The app may retain bounded before/after text and one recent attempt locally for up to 30 days, even when saving audio for training is off. At most 256 results are retained, with expiry applied when the store is next accessed. Undo learning removes that result’s analysis; Forget automatic learning also removes all of this analysis. No audio or training labels are created by it. See [semantic personalization](personalization/semantic-shadow.md) for details and current evidence.

### Formatting

**Format numbers, times, emails and links** (Writing, on by default) writes spoken forms the way you would type them: “two thousand five hundred dollars” → $2,500, “ten thirty a m” → 10:30 AM, “john dot smith at gmail dot com” → john.smith@gmail.com, “github dot com slash vesper” → github.com/vesper. It removes “um”, “uh” and repeated words (unless **Keep fillers and repetition** is on), and in “Tuesday no Wednesday” keeps only the corrected value. It works the same with every speech model and is English only. See [dictation formatting](personalization/formatting.md).

### Corrections in other applications

Two optional settings in Writing → Workspaces let Vesper learn from the app you dictate into. Both are off until you turn them on.

- **Use words already in the field:** when you dictate into a text field, Vesper reads the text before the cursor and uses names and terms written there to fix misheard words in that dictation. Nothing it reads is saved.
- **Learn from fixes in other apps:** after pasting, Vesper reads back only the text it pasted, at your next dictation or two minutes later. Words you changed there teach spellings the same way an edit in Vesper does, and the corrected text becomes that dictation's draft in History.

Neither reads password fields. Apps that do not expose their text show as unreadable in the per-app counts. See [learning from the app you dictate into](personalization/app-learning.md). Correcting the draft inside Vesper still teaches as before. The former TextEdit-only observer is described in the [removal note](personalization/external-observer.md).

**IBM Granite Speech 5.0 TurboCTC** is an optional fast model for Apple Silicon. It supports English, French, German, Spanish, Portuguese, Italian and Japanese, and writes punctuation. It runs only on the Mac's GPU: keep **Automatic** (or choose **Metal**), select its language, save, and use **Download** (about 948 MB). CPU is unavailable for this model, and it is not offered on other computers. See [its evaluation](validation/granite-speech5-turboctc-2026-10-21.md).

**Fermion Phonon-2 (English)** is the smallest Apple Silicon option: about 351 MB, with little GPU memory. It is somewhat less accurate than Parakeet or Granite, so choose it when disk space or memory matters most. Like Granite, it runs only on the Mac's GPU (Automatic or Metal). See [its evaluation](validation/phonon2-2026-10-21.md).

**Moondream Parakeet Redux** is the smallest multilingual option for Apple Silicon: about 217 MB, with Parakeet's 25 European languages and automatic detection. It is less accurate than Parakeet, especially in noise, so choose it when disk space or memory matters most. It runs only on the Mac's GPU (Automatic or Metal). See [its evaluation](validation/parakeet-redux-2026-10-21.md).

Apple Silicon builds can run **Parakeet** and **Qwen ASR** on the GPU automatically for **English**. Keep **Automatic**, save your model selection, and choose **Download**. Compatible GPU files can use more disk space than CPU files. Existing CPU downloads stay usable until the update is prepared; **Ready · update available** means you can transcribe now or download the optimized package. Removing a model removes its CPU and GPU exports. Full recordings and imported files share this pipeline; live partial transcription still requires Nemotron. Other languages and automatic language detection use their supported paths and may need a separate download. CPU remains an advanced override. See [accelerator setup](accelerators.md#building-the-apple-silicon-application).

Parakeet's Apple-GPU files were halved to about 1.25 GB on 22 October 2026, with identical transcripts and less memory ([details](offline-asr.md#parakeet-mlx-precision)). If you use Parakeet on the GPU, choose **Download** once to fetch the new files.

Parakeet's CPU files were replaced in October 2026 by a more accurate build of
the same model (about 730 MB; [details](offline-asr.md#parakeet-cpu-precision)).
If you use Parakeet on CPU, choose **Download** once to fetch the new files.

For these optional GPU models, a recording containing only exact digital zeros returns empty text after file verification without first loading the speech model. Quiet speech and ordinary microphone background noise still follow normal transcription.
