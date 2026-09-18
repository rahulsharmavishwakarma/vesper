# Using transcribe

For installation and first launch, see the [product README](../README.md) and
[development guide](development.md). See [model choices](model-presets.md) for
supported speech and writing models.

## Correct and copy a dictation

Edit **Your text** before copying when necessary. **Copy text** waits for your
edit to save and stays busy until copying finishes. Repeated clicks while a
save is pending do not queue additional copies. If saving fails, your edit
remains visible; use **Retry saving edit**, then copy again. The original
transcription remains available for comparison.

## Personal vocabulary

Eligible corrections in **Your text** contribute to local spelling memory
automatically. Personal vocabulary persists across restarts and model changes;
these corrections do not train model weights. Copying unchanged text does not
provide an additional learning label.

Manage vocabulary and writing preferences in **Settings → Your vocabulary and
writing**. **Undo learning** pauses an inferred spelling while keeping your
edited text. **Forget automatic learning** removes automatic memory while
preserving manual vocabulary; new recordings can teach again.

Corrections made after pasting into another application are not observed.
History review remains a separate workflow for explicitly reviewed targets.
Read the [personalization guide](personalization/README.md) and
[storage and privacy guide](personalization/storage-and-privacy.md) for the
learning, retention and deletion boundaries.
