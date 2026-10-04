# UI interaction design

Keep common actions familiar and direct. A control should let someone perform the task without assembling its implementation details. Preserve Vesper's shared Bricolage Grotesque typography, ember tokens, focus treatment, spacing, and approved motion; simplicity is an interaction requirement as well as a visual one.

Source comments explain behavior, constraints and asset ownership. Do not embed
design-tool names, self-review scores or generation-session stamps in production
components or styles. Keep useful notes about motion, focus and trusted artwork;
removing tool metadata must not change styles, markup, events or animation.

## Selected identity

Use **Vesper** for the product name in UI copy, documentation, issue templates,
GitHub issue text and project display metadata. Do not substitute it for the
transcription verb, third-party model names, executable identifiers or recorded
historical names. See [product naming and compatibility](development.md#product-name-and-compatibility).

**Ember** is the current Vesper identity, finalized by the owner in
[issue #87](https://github.com/rahulsharmavishwakarma/transcribe/issues/87).
The in-app lockup pairs the filled ember bird in
[its own SVG](../apps/desktop/src/assets/ember-bird.svg) with a Bricolage
Grotesque 800 wordmark, and every other application string uses the same
Bricolage Grotesque family — body and labels at 400/500, no monospace anywhere. The bird keeps two call dashes and an eye-lid highlight with an idle
chirp/blink motion and a hover tilt, all stilled by Reduce Motion. Native app
icons, the menu-bar template and the README banner still derive from the
Songbird source below until a follow-up requalifies that pipeline.

**A — Songbird** was the owner-selected Vesper identity, approved in
[issue #71](https://github.com/rahulsharmavishwakarma/transcribe/issues/71),
until Ember superseded it for in-app UI. Its selection and qualification are
preserved here as history.
The original bird, two call strokes and six individually drawn letter paths live
in [the canonical SVG](../apps/desktop/src/assets/vesper.svg). No logo library,
external font or downloaded artwork supplies this mark. The earlier web
similarity screening was preliminary, not a claim of worldwide exclusivity or
trademark clearance. Selecting the artwork does not select a distribution
license or establish that the Vesper name is legally available.

The repository README uses the Songbird wordmark in a still ember
banner: the Songbird bird/calls recolored ember on near-black, light lettering,
and generous clear space. All six
letters remain visible. The tagline and development status sit outside
the artwork as accessible, reflowing text. Keep this opening brief and direct;
do not add decorative animation, borrowed product artwork, fabricated badges or
claims of a release/license that does not exist. The banner is a derivative,
not a second logo source; see [brand asset generation](development.md#brand-assets).

Keep the product branding platform-neutral: **“Voice to text, on your device.”**
The header describes local models and early development, not a Mac-only identity.
Put current platform requirements in Download, setup and permission guidance;
macOS on Apple silicon remains the supported target today. Platform-neutral
wording does not claim other devices are supported or promise a release date.

README validation on 26 September 2026 used GitHub's document-mode Markdown
renderer and a local browser preview with approximate host typography. The
banner and header fit at 320/375/414/768/1200 px in light and dark surroundings,
with no horizontal overflow or split navigation labels. The original nine
paths match exactly; measured banner contrast is 11.16:1 for the bird and
15.97:1 for the lettering. This is local rendering evidence, not verification
of an unpublished asset on the hosted repository or a screen-reader audit.
Those screenshots precede the platform-neutral wording refinement; the artwork
and layout are unchanged, but the longer tagline was not requalified in them.

`BrandLogo.svelte` renders the Ember lockup at 160 px in the sidebar, 120 px in
the narrow-window header and 224 px in About, scaled by container width and
bounded by the available width. The accent token colours the bird; ink colours
the wordmark, which remains real text. The sidebar's existing button
retains its accessible destination and keyboard behavior; About replays the idle
call motion on opening. The compact capture strip
keeps readable text. The macOS menu bar uses only the still Songbird bird and its two
call strokes, with no wordmark or tile. Its transparent template lets macOS
choose the monochrome tint; the tooltip retains the Vesper name and activity.
Clicking continues to open the same native menu. Native app icons use a still
bird on a near-black rounded tile, generated from the Songbird source
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

That identifier describes the 23 September build. The current application uses
`com.vesper.desktop`, with legacy data lookup documented in
[development](development.md#product-name-and-compatibility). The selected
artwork and motion are unchanged by the technical namespace migration.

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

## Settings section navigation

The selected HTML Settings study is integrated into the application: an interior
sidebar for **Recording**, **Writing**, **Shortcuts**, **Appearance** and **About**,
with one shared content surface. Selected sections have a tonal background and
no underline. The native application sidebar keeps its existing treatment.
Sections use extra-bold Bricolage headings with natural tracking, a 1.4 line height
and shared description spacing; the main page title stays visually distinct.
At narrow widths the interior navigation becomes a wrapping row of 44 px targets.

Keep the real settings controls and their data ownership. Section changes hide
panels without recreating the application draft or dirty personal preferences.
Recording owns language, notifications, saved recordings and advanced storage
options. Writing owns cleanup and the independent vocabulary/preferences form.
Shortcuts keeps direct key capture and paste permissions. Application **Save
changes** saves its shared draft across sections; **Discard changes** restores
the last saved snapshot. Vocabulary retains its own Save/Discard actions. Restore
defaults is under Recording's advanced options and still explicitly saves defaults.

Appearance offers Light, Dark and System using the shared semantic tokens.
Light uses white cards over a warm cream canvas with a peach sidebar, espresso
text and tan control outlines. Reserve the ember accent for primary actions,
selections and status indicators; do not tint all backgrounds or secondary text
orange. Its appearance swatch uses the same canvas token value. Dark pairs a
near-black canvas with ember accents.
Keep primary text and supporting copy at least 4.5:1 against their surfaces,
and active control outlines and focus indicators at least 3:1. Palette revisions
must be inspected across Dictate, History, Models and Settings, including narrow
windows and hover, focus, selected, disabled and error states.
The 27 September 2026 light-palette check measured 14.68:1 for body text on
panels, 4.96:1 or higher for supporting text and primary-button labels, 3.07:1
or higher for control outlines on the canvas/panels, and 6.03:1 for focus on the
canvas. The production renderer build and five existing appearance/responsive
browser checks passed, including Settings at 320/360/375/414/768/1280 px. These
are local Chromium checks, not native display or screen-reader qualification.
The Ember palette (October 2026) measures 18.43:1 dark body and 17.45:1 light
body text, 6.33:1 dark and 4.94:1 light supporting text, 6.88:1 dark and
4.77:1 light primary-button labels, 3.32:1 dark and 3.10:1 light control
outlines, and 7.00:1 dark and 4.54:1 light focus. Ember deepens the light
accent, muted tone and both control outlines from the finalized mock values to
hold these bars; the hues are unchanged.
Selection applies immediately, defaults to the existing Dark appearance and is
stored under `vesper-appearance-v1` in the browser/webview's local storage,
separately from native inference settings. Other windows on the same origin sync
through storage events; System follows subsequent OS theme changes. A failed
storage write shows an error without changing the current choice. Clearing local
storage restores Dark. Reduce Motion follows the OS; this screen reports its
state rather than pretending to add a separate override. Floating capture options
remain normal application preferences and require Save changes.

About renders actual app metadata inline and shares its content and clipboard
recovery with the native About sheet. Neither surface exposes simulated updates.
The HTML study remains a review artifact for the other screens; their layouts
have not been integrated by this Settings change. `settings-layout.spec.mjs`
covers section drafts, separate saves, appearance persistence/system/error states
and all five sections in both themes at desktop and narrow widths.

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
full names in Settings → Recording and History. Names wrap within the available width;
do not hide either model or truncate it to make the window fit.

At the minimum window size, the first Settings control should be fully visible
without scrolling past a model summary. The Settings model summary follows its controls. The rest of each page uses
normal vertical scrolling, including Save. About is reached from its Settings section. Resizing must preserve
unsaved values; open dropdowns close on resize and can reopen within the new
viewport. Dialogs remain height-constrained and scroll internally as needed.

Check short heights as well as narrow widths, including both sides of the
700 px sidebar breakpoint. `tests/responsive.spec.mjs` uses synthetic data to
exercise the minimum window, long model names, navigation, expanded History,
save reachability, dropdowns, dialog bounds and draft preservation on resize.

## Shortcut recording

Settings → Shortcuts has one shortcut field for each action. Click the field (or focus it and activate it with Enter/Space), press the desired combination, then use **Save changes**. The field displays the captured combination with platform-appropriate modifier names. A valid capture changes only the draft. Escape restores the values present when editing began; Tab, clicking away, or switching windows stops recording keys and keeps a valid draft available to save. Restore defaults remains the existing settings action.

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
Settings → About shows app identity/version, platform details, a copy icon and a
compact update status in the content panel. Native About menu entries open the
**A03 About sheet**, reusing the same content. Both preserve shared controls and
error recovery. Metadata comes from `get_app_info` independently of model/settings
readiness. A release build is not labeled as a verified stable release channel.

Group version, build type and platform together. Copy uses the shared icon-button
control with a tooltip and accessible name, a check on success, and selected text
for clipboard failure recovery. Keep normal-state action wording out of the
visible layout. The unavailable-update notice is one sentence below Software
updates, without a large warning headline or repeated explanation.

Keep About outside the main application sidebar and editable forms. Its Settings
section preserves unsaved drafts; native About remains available when settings
cannot load and restores keyboard focus when closed. This follows
[Raycast's About placement within Settings](https://manual.raycast.com/settings)
and [VS Code's native About entry](https://code.visualstudio.com/docs/supporting/faq).

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
