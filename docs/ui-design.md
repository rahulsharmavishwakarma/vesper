# UI interaction design

Keep common actions familiar and direct. A control should let someone perform the task without assembling its implementation details. Preserve Vesper's shared typography, graphite/sage tokens, focus treatment, spacing, and approved motion; simplicity is an interaction requirement as well as a visual one.

## Selected identity

Use **Vesper** for the product name in UI copy, documentation, issue templates,
GitHub issue text and project display metadata. Do not substitute it for the
transcription verb, third-party model names, executable identifiers or recorded
historical names. See [product naming and compatibility](development.md#product-name-and-compatibility).

**A — Songbird** is the owner-selected Vesper identity, approved in
[issue #71](https://github.com/rahulsharmavishwakarma/transcribe/issues/71).
The original bird, two call strokes and six individually drawn letter paths live
in [the canonical SVG](../apps/desktop/src/assets/vesper.svg). No logo library,
external font or downloaded artwork supplies this mark. The earlier web
similarity screening was preliminary, not a claim of worldwide exclusivity or
trademark clearance. Selecting the artwork does not select a distribution
license or establish that the Vesper name is legally available.

`BrandLogo.svelte` renders that same source at 160 px in the sidebar, 120 px in
the narrow-window header and 224 px in About, bounded by the available width.
Existing sage/ink tokens provide its colours. The sidebar's existing button
retains its accessible destination and keyboard behavior; About exposes the SVG
as “Vesper.” The compact capture strip
keeps readable text. The macOS menu bar uses only the still bird and its two
call strokes, with no wordmark or tile. Its transparent template lets macOS
choose the monochrome tint; the tooltip retains the Vesper name and activity.
Clicking continues to open the same native menu. Native app icons use a still
bird on the existing graphite rounded tile, generated from the same source
rather than redrawn.

Only About opening triggers a finite, staggered reveal of the two call strokes.
The sidebar, bird body and letters never animate. Motion changes only call
opacity and translation; no pulsing, scale, bouncing, looping or input delay is
introduced. Closing clears the activation, reopening replays it, and OS Reduce
Motion leaves the complete mark visible. See [motion design](motion-design.md#brand-motion)
and [icon generation](development.md#brand-assets) for the implementation contract.

Identity verification on 23 September 2026: 50 Chromium browser regressions and
117 Node helper/tool tests passed, including full-name geometry and compatibility
checks. The logo/About sheet fit at 320/375/414/768/1180/1440 px without horizontal
overflow. Pointer and actual Enter/Escape focus recovery, eight sampled motion
frames, reopen, initial/live Reduce Motion and no preference-toggle replay were
checked. Computed static contrast against sidebar/About surfaces measured
10.44:1 for the bird and 14.95:1 for the wordmark. Frontend lint, type check,
formatting and production build passed.
The macOS arm64 debug bundle built as `Vesper.app`; its plist retained
`com.transcribe.desktop`, and its icon matched the reproducibly generated ICNS
that macOS `iconutil` decoded successfully. These checks did not launch the app,
qualify native Dock appearance, exercise a screen reader, or validate microphone,
inference, release distribution or Windows/Linux behavior.

The identity regression navigates to Settings and activates a visible About
entry, rather than assuming where that entry is rendered. Brand checks therefore
remain independent of sidebar/Settings presentation changes; native calls are
still mocked.

## Before changing a control

For a new or unfamiliar interaction, consult the relevant platform guidance and one or two established applications. Identify what users already know how to do, how many actions the flow needs, how it confirms success, and how it handles failure or cancellation. Record the sources and the chosen adaptation in the GitHub issue. A reference establishes a usable precedent, not this app's runtime capabilities. Use the existing component and service boundaries wherever possible.

Prefer a direct field or action over a builder, nested panel, extra confirmation, or advanced terminology. Add another step only when it helps the person make a necessary decision or recover from a real failure. Existing Save behavior should remain consistent across the screen; do not add a preliminary confirmation just to move a value into the same settings draft.

Main views start with their page heading. Do not add a workspace breadcrumb or a
Local workspace badge: the app has no workspace navigation for those labels to
describe. The existing sidebar selection provides navigation context; operation
feedback and the browser-preview notice retain their own meaningful states.

On Models, keep selected model names and file actions in their existing model
cards. Local engine shows resources, readiness and operation details without
repeating both selected models. Do not add a separate batch-verification card;
automatic integrity checks and each model's Download/Verify files action own that
flow. Keep these removals independent of changes to page width or card layout.

## Small windows

The main window supports 360 × 480 logical pixels. At widths up to 700 px,
navigation becomes a compact header: reduce its surrounding space and wordmark
size while retaining 44 px button targets. Keep the same page order and cards.
Use tighter heading/card spacing and put the saved model labels beside their
full names in Settings and History. Names wrap within the available width;
do not hide either model or truncate it to make the window fit.

At the minimum window size, the first Settings control should be fully visible
without scrolling past the header and model summary. The rest of each page uses
normal vertical scrolling, including Save and About. Resizing must preserve
unsaved values; open dropdowns close on resize and can reopen within the new
viewport. Dialogs remain height-constrained and scroll internally as needed.

Check short heights as well as narrow widths, including both sides of the
700 px sidebar breakpoint. `tests/responsive.spec.mjs` uses synthetic data to
exercise the minimum window, long model names, navigation, expanded History,
save reachability, dropdowns, dialog bounds and draft preservation on resize.

## Shortcut recording

Settings → Quick capture has one shortcut field for each action. Click the field (or focus it and activate it with Enter/Space), press the desired combination, then use **Save changes**. The field displays the captured combination with platform-appropriate modifier names. A valid capture changes only the draft. Escape restores the values present when editing began; Tab, clicking away, or switching windows stops recording keys and keeps a valid draft available to save. Restore defaults remains the existing settings action.

The recorder provides a preparing state before native capture is ready, inline unsupported/duplicate feedback, visible keyboard focus, and disabled behavior during an operation. Modifier selection buttons, key dropdowns, the Customize panel, and the extra Use shortcut step are not part of this flow. A shortcut recorder must not start the action it is configuring: the existing native callback routes already-registered combinations to the active main-window editor, while browser key events capture other supported combinations. Editing is session-scoped and stops on blur/close; late callbacks cannot change another field. No additional global keyboard hook is introduced.

The current binding contract is modifier + supported physical key, as described in [development.md](development.md#separate-hold-and-toggle-shortcuts). Other products' Fn, modifier-only, multi-step, or character-layout support does not imply those capabilities exist here. Native registration remains authoritative for errors it can detect; this UI does not claim to enumerate all other applications' shortcuts.

### Official interaction references

- [Apple: customise keyboard shortcuts](https://support.apple.com/en-gb/guide/mac-help/mchlp2262/mac) edits the existing shortcut by activating its field and pressing the replacement combination. Its guidance also covers conflicts and restoring defaults.
- [Raycast: Command Aliases & Hotkeys](https://manual.raycast.com/command-aliases-and-hotkeys) uses a shortcut recorder with immediate key display and conflict feedback. Vesper retains explicit Save changes rather than adopting Raycast's automatic saving.
- [VS Code: keyboard shortcuts](https://code.visualstudio.com/docs/configure/keybindings) documents pressing the intended combination to define a binding and inspecting conflicting assignments.

These are interaction precedents, not evidence that Vesper implements the products' native key-capture internals or their full key support.

## About and updates studies

The [standalone HTML samples](design/about-updates/index.html) compare three
proposed layouts: A01 Quiet settings, A02 Split view and A03 About sheet. See the
[preview guide](design/about-updates/README.md) for the interactive states and
selection export. These are local review artifacts in the ignored `docs/design/`
directory, consistent with the repository's treatment of exploratory studies.
The implemented direction is **A03 About sheet**. A full-width
**About Vesper** entry at the end of Settings uses the same panel surface,
border, radius, padding and heading typography as the preference sections.
It and the native About entries open the same modal with app identity/version,
platform details, a copy icon and a compact update status. It preserves the shared graphite/sage controls, keyboard
dismissal and focus recovery demonstrated in the sample. Metadata comes from
`get_app_info`, independently of model/settings readiness. A release build is
not labeled as a verified stable release channel.

Group version, build type and platform together. Copy uses the shared icon-button
control with a tooltip and accessible name, a check on success, and selected text
for clipboard failure recovery. Keep normal-state action wording out of the
visible layout. The unavailable-update notice is one sentence below Software
updates, without a large warning headline or repeated explanation.

Keep About outside the main sidebar and below all editable preferences. The row
is independent of Save changes, preserves unsaved drafts, remains available if
settings cannot load, and returns keyboard focus after closing the sheet.
This follows [Raycast's About placement within Settings](https://manual.raycast.com/settings)
and [VS Code's native About entry](https://code.visualstudio.com/docs/supporting/faq),
adapted to Vesper's existing controls rather than adding a new navigation tab.

The samples use illustrative release data and simulated operations. The real
sheet reports that automatic updates are unavailable and provides no simulated
download controls. Update controls depend on qualified distribution. Installation must wait for
active work to finish, and dismissal must not imply native download cancellation.
Keep technical signing/feed details out of the ordinary user flow. The proposed
interaction and implementation limits are tracked in issues
[#66](https://github.com/rahulsharmavishwakarma/transcribe/issues/66) and
[#65](https://github.com/rahulsharmavishwakarma/transcribe/issues/65).

## Verification

Exercise the whole task with pointer and keyboard input: initial value, entering edit mode, valid and invalid input, cancellation, focus movement, saving, failure/retry, and reloading. Include slow asynchronous setup and navigation while work is pending. Check narrow and desktop layouts using the rendered component; do not rely solely on a screenshot or a synthetic event pass. Reduced motion must preserve the same state and feedback. Keep real OS shortcut and permission qualification separate from browser mocks.
