# Using transcribe

transcribe is a desktop dictation app for Apple-silicon Macs. It transcribes
microphone recordings and audio files locally, with optional text cleanup,
recording history and personal vocabulary.

Start at the [product README](../README.md) for download status or follow the
[development guide](development.md) to run from source. The public DMG and Apple
Developer ID signing/notarization are not available yet. Once a release is
available, open its DMG and drag `transcribe.app` to Applications.

## First launch

1. Open **Models** and select speech and writing models. The writing model is
   used only when text cleanup is enabled.
2. Download the selected files. **Save & download** saves a pending selection
   before downloading it; **Save changes** alone does not download anything.
3. Open **Dictate**, select your microphone and choose a language supported by
   the speech model. Allow microphone access when macOS asks.
4. Wait until the app is ready before recording. The first recording may take
   longer while a model loads.

Model downloads need internet access and additional disk space. Files are
verified before use. Manage installed models, the model folder and device
acceleration in **Models**. See [model choices](model-presets.md) for details.

New installations use Cohere Transcribe with optional Qwen writing cleanup and
English selected. The current Cohere adapter supports English and French for
completed recordings; it does not provide automatic language detection, Hindi
or live captions. Other models have different capabilities. Use the
[language compatibility guide](language-support-and-recommendations.md) when
changing models rather than assuming every model supports every mode.

## Record or import audio

In **Dictate**, choose **Record**, wait for **Listening**, and speak. Choose
**Stop and transcribe** when finished. To transcribe an existing recording, use
the audio-file action instead. Review the result before using it.

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

## Quick capture

The desktop app must be running for its global shortcuts to work.

| Action | Default Mac shortcut | Behavior |
| --- | --- | --- |
| Hold to talk | `⌥ Shift D` | Hold while speaking; release to finish. |
| Press to toggle | `⌘ Shift Space` | Press once to start and again to finish. |

Both shortcuts can be changed in **Settings → Quick capture**. Existing
installations keep their saved shortcuts. Wait for the floating controls to
show **Listening** before speaking.

To insert a finished transcript into another app, select **Paste into the
original text field** in Quick capture settings, enable the requested macOS
Accessibility access and save. Focus the intended field before recording.
Insertion depends on the target app and its focus state; if it is unavailable,
use **Copy**. External-app compatibility is still being qualified.

You can change the capture design, move the floating controls, or hide them
while keeping shortcuts active. The menu-bar **transcribe** item provides
recording status and quick access to History, Models, Settings and Quit.
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

| Problem | First checks |
| --- | --- |
| Recording will not start | Confirm microphone permission, selected input device and model download status. Wait for Listening. |
| A language or live captions are unavailable | Check the selected speech model's [supported languages and modes](language-support-and-recommendations.md). |
| A model download fails | Check the connection, free disk space and model-folder access, then retry from Models. |
| A shortcut is unavailable | Resolve the conflict shown beside the shortcut setting or choose another combination. |
| Automatic paste fails | Check Accessibility permission and the intended text field; use Copy if insertion is unavailable. |
| A recording is missing from History | Check whether Save recordings and transcripts was enabled for that session. |

If the problem persists, follow [Support](../SUPPORT.md). Include the application
version, macOS version, selected model and a synthetic reproduction. Do not
include private audio, transcripts or credentials.
