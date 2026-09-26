# Using Vesper

Vesper is a desktop dictation app for Apple-silicon Macs. It transcribes
microphone recordings and audio files locally, with optional text cleanup,
recording history and personal vocabulary.

Start at the [product README](../README.md) for download status or follow the
[development guide](development.md) to run from source. The public DMG and Apple
Developer ID signing/notarization are not available yet. Once a release is
available, open its DMG and drag `Vesper.app` to Applications. The repository and
download filename still use `transcribe`; existing settings and model locations
are unchanged by the display-name update. Use **Vesper** when referring to the
app in documentation and issues; [technical compatibility names](development.md#product-name-and-compatibility)
do not indicate a different application.

## First launch

### Small windows

When the window is narrow, navigation moves to the top and spacing becomes
more compact. Both configured model names remain visible in Settings and
History. Scroll vertically to reach the remaining controls, Save actions and
About. Resizing preserves unsaved preferences; an open dropdown closes and can
be reopened in its new position.

### App information

Open **Settings** and choose **About Vesper** at the bottom of the page,
or choose **About Vesper…** in the native menu, to view the About sheet.
The selected custom Songbird logo spells all six letters in the sidebar and
About sheet; the app icon uses the bird alone. Only opening About animates its
two small call marks once. The bird and letters stay still, and Reduce Motion
disables that decoration.
It shows the compiled application version, development/release build type and
platform. The copy icon beside those details (tooltip: **Copy app info**) copies
them for sharing in a bug report; it does not include
recordings, transcripts or local paths. If clipboard access fails, the sheet
provides selected text for manual copying. Escape or Close returns you to the
previous screen without changing your settings.

Automatic application updates are not connected yet. The sheet reports this
instead of offering a check/download/restart action. Model downloads continue
to be managed separately in **Models**.

### Initial setup

1. Open **Models** and select speech and writing models. The writing model is
   used only when text cleanup is enabled.
2. Download the selected files. **Save & download** saves a pending selection
   before downloading it; **Save changes** alone does not download speech or writing
   weights. Supporting audio packages download automatically when those models are ready.
3. Open **Dictate**, select your microphone and choose a language supported by
   the speech model. Allow microphone access when macOS asks.
4. Wait until the app is ready before recording. The first recording may take
   longer while a model loads.

Model downloads need internet access and additional disk space. Files are
verified before use. Manage installed models, the model folder and device
acceleration in **Models**. See [model choices](model-presets.md) for details.

Each model card shows its selected model and download status. **Local engine**
above it shows readiness, CPU, memory and operation timings. Models are checked
automatically when loaded; the separate **Selected models** verification card is
not needed. If model files are missing or damaged, use **Download** or **Verify
files** on the affected model. Verification keeps valid files and downloads
replacements for missing or damaged files. Repairs require internet access and
are started by this action, not automatically during transcription.

**Automatic** uses a compatible detected accelerator where the selected runtime
supports it. LFM Audio and the writing model share native GPU selection; on Apple
Silicon this can use Metal. If GPU initialization fails in Automatic mode, the
runtime retries on CPU and records the reason. An explicitly selected GPU fails
when unavailable instead of silently switching devices. The first GPU request
can take longer while native kernels initialize.

GPU support is specific to the model and runtime. On Apple Silicon, Automatic chooses the compatible GPU files for English Parakeet and Qwen ASR when this app includes the verified runtime and detects a usable GPU. Save your selection and use Download; no backend choice is required. Cohere and Whisper Turbo retain their CPU speech paths. The writing model makes its own accelerator selection. Whisper remains outside the desktop model picker.
See [performance evidence](inference-performance.md)
for tested combinations and remaining limits.

New installations use Cohere Transcribe with optional Qwen writing cleanup and
English selected. The current Cohere adapter supports English and French for
completed recordings; it does not provide automatic language detection, Hindi
or live captions. Other models have different capabilities. Use the
[language compatibility guide](language-support-and-recommendations.md) when
changing models rather than assuming every model supports every mode.

Model defaults are reviewed against measured accuracy, failures and performance
for the intended use. A newer release alone is not a reason to replace a model.
The current selection remains provisional; see the [benchmark selection policy](model-presets.md#choosing-the-default-from-benchmarks).

## Record or import audio

In **Dictate**, choose **Record**, wait for **Listening**, and speak. Choose
**Stop and transcribe** when finished. To transcribe an existing recording, use
the audio-file action instead. Review the result before using it.

The waveform follows microphone level and estimated voice pitch. Quiet input
produces smaller strokes; unvoiced sounds still register without forcing the
spacing to jump. It is a recording indicator, not a speech/noise detector: fans,
music and other audible sounds can also move it.

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

### Experimental audio processing

In **Models → Audio processing**, supporting audio models are managed by the app.
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
uses the existing installation operation, so recording waits during an active
download; cancellation lets you continue with original-audio fallback. Files are
verified against pinned hashes. Downloads require internet access; transcription
and audio processing run entirely on device after setup.

Supporting files are checked automatically once per app session and model folder.
Valid stored files are reused; missing or corrupt files are downloaded and repaired.
No audio-model selection, download, verification or retry action is needed.
After a failed or cancelled transfer, setup retries after 30 seconds, then
2 minutes, with later retries spaced 10 minutes apart; reconnecting to the internet
also resumes setup. It always waits for recording and settings edits to finish.
Changing and saving the model folder checks the new location. ASR and writing
weights still use **Save & download**.

Noise reduction applies to live transcription and completed recordings. It can
reduce transcription accuracy, especially for quiet speech; the existing
[denoising comparisons](validation/denoising-followup-2026-09-19.md) remain mixed.
This fixed product choice is not a claim of improved accuracy for every ASR model
or accent. Original recordings remain intact. Missing models or processing
failures use the original-audio fallback; failures are reported with a warning.

### Experimental speaker timelines

Speaker analysis runs automatically when its local model and runtime are
available; there is no enable switch or settings-save step. Its model downloads
automatically once the selected speech/writing models and local speaker runtime
are ready. **Models → Audio processing** shows its setup status.
This experimental integration requires Apple silicon and the
[locally built speaker runtime](development.md#experimental-speaker-runtime);
there is no newly published runtime installer. It runs entirely on device.

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

Both shortcuts can be changed in **Settings → Quick capture**. Existing
installations keep their saved shortcuts. Wait for the floating controls to
show **Listening** before speaking.

To insert a finished transcript into another app, select **Paste into the
original text field** in Quick capture settings, enable the requested macOS
Accessibility access and save. Focus the intended field before recording.
Hold release or Toggle Stop then inserts the finished transcript after
transcription and writing complete. Keep the original field and caret selected
until it finishes. **Keep in app** deliberately disables automatic paste;
main-window Record and imported files also keep their result in the app.

Check **Ready to paste** in Quick capture settings before recording. On macOS
versions that request clipboard access, allow automatic clipboard access so
Vesper can preserve your clipboard without opening a prompt during delivery.
With automatic paste enabled, Vesper prepares supported Electron editors such
as VS Code when you switch to them. A freshly opened editor can take a moment
to expose its text field; if it is not ready when recording starts, that result
remains available through **Copy**. Changing to **Keep in app** stops this
preparation. Protected fields, Terminal, iTerm and Warp remain excluded.
External-app compatibility is still being qualified.

Live words in the floating recorder require a streaming speech model. The
current **NVIDIA Parakeet TDT 0.6B v3** adapter produces text after Stop;
**NVIDIA Nemotron 3.5 ASR Streaming 0.6B** supports live preview. Select your
model on **Models** and wait for **Listening** before speaking, particularly
after cold model startup. Live words stay in the recorder; the external field
receives one final paste after release.

You can change the capture design, move the floating controls, or hide them
while keeping shortcuts active. On macOS, click the **bird logo** in the menu
bar for recording status and quick access to History, Models, Settings and Quit.
It shows only the still logo, without the Vesper wordmark or a background tile;
macOS adjusts its monochrome appearance. Hover for the Vesper name and status.
Closing the main window keeps the app running. Finish or cancel active work
before quitting.

## History and personal vocabulary

**History** contains saved recordings and transcripts. Search by text or
filename, filter by review status or date, and expand a recording to review or
copy its text. Export options distinguish a complete archive from reviewed
training examples; search filters do not limit an export.

Manage spellings, recurring corrections and writing preferences in
**Settings → Your vocabulary and writing**. Eligible edits to **Your text** can
contribute to local spelling memory for later recordings. Raw predictions remain
available, and remembered changes can be inspected, undone or forgotten.

Correct spellings directly in **Your text**; there is no separate Remember step.
The app can learn eligible joined/split names, closely related spellings and
narrow acronym corrections, even when the edit includes minor formatting.
Ordinary terms need corrections from two independent recordings before they
are reused in matching context. Replaying the same recording does not count
twice. Sentence rewrites and changed decisions are not global dictionary rules,
and some valid corrections still fall outside the current learning rules.

If you restore an earlier spelling in the same draft, its withdrawn correction
no longer counts as supporting evidence. Correcting an automatically remembered
word to another eligible spelling pauses the rejected rule. Fresh corrections
of the original recognition error can then teach its replacement; the app keeps
your current draft immediately while waiting for that support.

Personal vocabulary persists across model changes. These local corrections do
not train model weights or activate a new model. Read the
[personalization guide](personalization/README.md) for the exact learning,
review and undo behavior.

Automatically learned words are used after recognition only when the transcript
also matches context from a supporting edit. Older learned words may need a fresh
correction before they apply again. This helps avoid unrelated replacements, but
can miss a valid correction when you phrase the context differently. Manually
saved vocabulary and optional recognition hints keep their existing controls.

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

| Problem                                     | First checks                                                                                                                                                                                              |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Waveform does not match the microphone      | Confirm the selected input device. After a native audio update, quit the running app and open the newly built version; a page reload does not update the audio code. Background sound can still register. |
| Recording will not start                    | Confirm microphone permission, selected input device and model download status. Wait for Listening.                                                                                                       |
| A language or live captions are unavailable | Check the selected speech model's [supported languages and modes](language-support-and-recommendations.md).                                                                                               |
| A model download fails                      | Check the connection, free disk space and model-folder access, then retry ASR/writing downloads from Models. Supporting audio setup retries automatically.                                                                                                                    |
| A shortcut is unavailable                   | Resolve the conflict shown beside the shortcut setting or choose another combination.                                                                                                                     |
| Automatic paste fails                       | Choose **Paste into the original text field**, save, and check **Ready to paste**. Keep the original field focused. A newly opened Electron editor may need a moment before the first recording; use Copy for an unavailable destination.                                                                                                         |
| A recording is missing from History         | Check whether Save recordings and transcripts was enabled for that session.                                                                                                                               |

If the problem persists, follow [Support](../SUPPORT.md). Include the application
version, macOS version, selected model and a synthetic reproduction. Do not
include private audio, transcripts or credentials.

## Background personalization analysis

When you save a correction inside Vesper, the app also attempts a small local analysis while idle, using the writing model only if it is already loaded. Recording interrupts this work. This experimental analysis helps evaluate intent and context; its guesses do not yet change your words or preferences. Automatic spelling learning continues as before.

The app may retain bounded before/after text and one recent attempt locally for up to 30 days, even when saving audio for training is off. At most 256 results are retained, with expiry applied when the store is next accessed. Undo learning removes that result’s analysis; Forget learned words also removes all of this analysis. No audio or training labels are created by it. See [semantic personalization](personalization/semantic-shadow.md) for details and current evidence.

### Optional TextEdit correction evaluation

In **Settings → Storage and improvement**, **Evaluate corrections in TextEdit** enables a separate local evaluation after automatic paste. It is off by default. The first adapter requires plain-text TextEdit with English menus on Mac and a direct keyboard layout. It observes the inserted text and up to 32 adjacent UTF-16 units on each side for at most one minute; it may abstain when the field, selection or boundaries change. Only inserted/edited text is retained locally, at most 256 records for 30 days. These candidates do not teach your dictionary yet. Disabling stops observation; **Forget learned spellings** also deletes retained candidates. Ordinary in-app spelling learning remains automatic. See the [full scope and limitations](personalization/external-observer.md).

Apple Silicon builds can run **Parakeet** and **Qwen ASR** on the GPU automatically for **English**. Keep **Automatic**, save your model selection, and choose **Download**. Compatible GPU files can use more disk space than CPU files. Existing CPU downloads stay usable until the update is prepared; **Ready · update available** means you can transcribe now or download the optimized package. Removing a model removes its CPU and GPU exports. Full recordings and imported files share this pipeline; live partial transcription still requires Nemotron. Other languages and automatic language detection use their supported paths and may need a separate download. CPU remains an advanced override. See [accelerator setup](accelerators.md#building-the-apple-silicon-application).

For these optional GPU models, a recording containing only exact digital zeros returns empty text after file verification without first loading the speech model. Quiet speech and ordinary microphone background noise still follow normal transcription.
