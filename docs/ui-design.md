# UI interaction design

Keep common actions familiar and direct. A control should let someone perform the task without assembling its implementation details. Preserve transcribe's shared typography, graphite/sage tokens, focus treatment, spacing, and approved motion; simplicity is an interaction requirement as well as a visual one.

## Before changing a control

For a new or unfamiliar interaction, consult the relevant platform guidance and one or two established applications. Identify what users already know how to do, how many actions the flow needs, how it confirms success, and how it handles failure or cancellation. Record the sources and the chosen adaptation in the GitHub issue. A reference establishes a usable precedent, not this app's runtime capabilities. Use the existing component and service boundaries wherever possible.

Prefer a direct field or action over a builder, nested panel, extra confirmation, or advanced terminology. Add another step only when it helps the person make a necessary decision or recover from a real failure. Existing Save behavior should remain consistent across the screen; do not add a preliminary confirmation just to move a value into the same settings draft.

## Shortcut recording

Settings → Quick capture has one shortcut field for each action. Click the field (or focus it and activate it with Enter/Space), press the desired combination, then use **Save changes**. The field displays the captured combination with platform-appropriate modifier names. A valid capture changes only the draft. Escape restores the values present when editing began; Tab, clicking away, or switching windows stops recording keys and keeps a valid draft available to save. Restore defaults remains the existing settings action.

The recorder provides a preparing state before native capture is ready, inline unsupported/duplicate feedback, visible keyboard focus, and disabled behavior during an operation. Modifier selection buttons, key dropdowns, the Customize panel, and the extra Use shortcut step are not part of this flow. A shortcut recorder must not start the action it is configuring: the existing native callback routes already-registered combinations to the active main-window editor, while browser key events capture other supported combinations. Editing is session-scoped and stops on blur/close; late callbacks cannot change another field. No additional global keyboard hook is introduced.

The current binding contract is modifier + supported physical key, as described in [development.md](development.md#separate-hold-and-toggle-shortcuts). Other products' Fn, modifier-only, multi-step, or character-layout support does not imply those capabilities exist here. Native registration remains authoritative for errors it can detect; this UI does not claim to enumerate all other applications' shortcuts.

### Official interaction references

- [Apple: customise keyboard shortcuts](https://support.apple.com/en-gb/guide/mac-help/mchlp2262/mac) edits the existing shortcut by activating its field and pressing the replacement combination. Its guidance also covers conflicts and restoring defaults.
- [Raycast: Command Aliases & Hotkeys](https://manual.raycast.com/command-aliases-and-hotkeys) uses a shortcut recorder with immediate key display and conflict feedback. transcribe retains explicit Save changes rather than adopting Raycast's automatic saving.
- [VS Code: keyboard shortcuts](https://code.visualstudio.com/docs/configure/keybindings) documents pressing the intended combination to define a binding and inspecting conflicting assignments.

These are interaction precedents, not evidence that transcribe implements the products' native key-capture internals or their full key support.

## Verification

Exercise the whole task with pointer and keyboard input: initial value, entering edit mode, valid and invalid input, cancellation, focus movement, saving, failure/retry, and reloading. Include slow asynchronous setup and navigation while work is pending. Check narrow and desktop layouts using the rendered component; do not rely solely on a screenshot or a synthetic event pass. Reduced motion must preserve the same state and feedback. Keep real OS shortcut and permission qualification separate from browser mocks.
