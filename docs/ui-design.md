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
[issue #87](https://github.com/rahulsharmavishwakarma/vesper/issues/87).
The in-app lockup pairs the filled ember bird in
[its own SVG](../apps/desktop/src/assets/ember-bird.svg) with a Bricolage
Grotesque 800 wordmark, and every other application string uses the same
Bricolage Grotesque family — body and labels at 400/500, no monospace anywhere. The bird keeps two call dashes and an eye-lid highlight with an idle
chirp/blink motion and a hover tilt, all stilled by Reduce Motion. Native app
icons, the menu-bar template and the README banner still derive from the
Songbird source below until a follow-up requalifies that pipeline.

**A — Songbird** was the owner-selected Vesper identity, approved in
[issue #71](https://github.com/rahulsharmavishwakarma/vesper/issues/71),
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

## Disclosure chevron convention

Every `<details>`/`<summary>` disclosure (recordings, export, learning, diagnostics, and the bordered rows inside `.preference-group`/`.settings-advanced` such as Vocabulary's "How does this work?", "Advanced storage and improvement", "Advanced writing instructions", "Restore default preferences") shares one CSS-drawn chevron built from `details > summary::after` in `apps/desktop/src/app.css`. The bordered variant switches `summary` to a flex row (`display: flex; align-items: center; justify-content: space-between`) so the chevron is meant to be centered as a normal flex child; that rule must also set `position: static` on the chevron, or the `position: absolute` from the plain (unbordered) convention leaks in, pulls the chevron out of flow, and leaves it pinned near the top of the row instead of centered (issue #128). When adding a new disclosure, reuse this convention rather than a new chevron, and verify the chevron is vertically centered in a rendered screenshot, not just by reading the CSS.

When a report-style panel needs optional methodology or caveat prose that most users will not read (for example, how a comparison is computed), nest a second, un-classed `<details>`/`<summary>` inside the panel's own disclosure rather than inlining the prose as visible `<p class="note">` text or inventing a tooltip/popover. `details > summary::after` is a generic descendant selector, so the nested disclosure automatically gets the same chevron with no extra CSS. `LearningSummary.svelte`'s "How this is measured" block (inside the History tab's "Learning on this device" report, issue #133) follows this pattern: it keeps headline stats, the review-queue actions, and the comparison table visible by default, and hides only the multi-sentence methodology paragraph behind the nested disclosure.

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
Shortcuts keeps direct key capture and paste permissions. Application edits
autosave their shared draft across sections about a second after each change;
a transient sidebar pill (Saving… sliding in, tick with save time, sliding
away) confirms each write, and explicit actions such as model downloads flush
pending edits first. There is no Save or Discard action. Vocabulary saves
through the same mechanism. Restore
defaults is under Recording's advanced options and still explicitly saves defaults.

Appearance offers Light, Dark and System using the shared semantic tokens.
Light uses white cards over a white canvas with a warm off-white sidebar,
near-black text and hairline control outlines. Reserve the ember accent for primary actions,
selections and status indicators; do not tint all backgrounds or secondary text
orange. Its appearance swatch uses the same canvas token value. Dark pairs a
near-black canvas with ember accents.
Keep primary text and supporting copy at least 4.5:1 against their surfaces,
and active control outlines and focus indicators at least 3:1. Palette revisions
must be inspected across Dictate, History, Models and Settings, including narrow
windows and hover, focus, pressed, selected, disabled and error states.
A press answers with a deeper wash while held on targets that must stay still
(sidebar nav, segmented options, history cards, listbox options); plain buttons
keep the small press scale. Icon-bearing labels stay flex so the glyph centers
with the text. Settings copy must never describe saving as a user action:
writes are automatic, confirmed only by the transient sidebar pill.
The 27 September 2026 light-palette check measured 14.68:1 for body text on
panels, 4.96:1 or higher for supporting text and primary-button labels, 3.07:1
or higher for control outlines on the canvas/panels, and 6.03:1 for focus on the
canvas. The production renderer build and five existing appearance/responsive
browser checks passed, including Settings at 320/360/375/414/768/1280 px. These
are local Chromium checks, not native display or screen-reader qualification.
The Ember v2 palette (October 2026) uses the finalized mock values exactly. It
measures 18.43:1 dark body and 18.87:1 light body text, 6.33:1 dark and
5.49:1 light supporting text, 6.88:1 dark and 3.81:1 light primary-button
labels, 3.32:1 dark and 3.58:1 light control outlines, and 7.00:1 dark and
3.57:1 light focus. Only light primary-button labels sit under the 4.5:1
bar; that matches the owner-finalized mock, so that pair is a known,
accepted limitation. Accent text stays on large, bold or non-text roles,
and focus/outline indicators clear 3:1.
Selection applies immediately, defaults to the existing Dark appearance and is
stored under `vesper-appearance-v1` in the browser/webview's local storage,
separately from native inference settings. Other windows on the same origin sync
through storage events; System follows subsequent OS theme changes. A failed
storage write shows an error without changing the current choice. Clearing local
storage restores Dark. Reduce Motion follows the OS; this screen reports its
state rather than pretending to add a separate override. Floating capture options
remain normal application preferences and autosave like everything else.

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

Settings → Shortcuts has one shortcut field for each action. Click the field (or focus it and activate it with Enter/Space), press the desired combination; it autosaves. The field displays the captured combination with platform-appropriate modifier names. A valid capture changes only the draft. Escape restores the values present when editing began; Tab, clicking away, or switching windows stops recording keys and keeps a valid draft, which autosaves. Restore defaults remains the existing settings action.

The recorder provides a preparing state before native capture is ready, inline unsupported/duplicate feedback, visible keyboard focus, and disabled behavior during an operation. Modifier selection buttons, key dropdowns, the Customize panel, and the extra Use shortcut step are not part of this flow. A shortcut recorder must not start the action it is configuring: the existing native callback routes already-registered combinations to the active main-window editor, while browser key events capture other supported combinations. Editing is session-scoped and stops on blur/close; late callbacks cannot change another field. No additional global keyboard hook is introduced.

The current binding contract is modifier + supported physical key, as described in [development.md](development.md#separate-hold-and-toggle-shortcuts). Other products' Fn, modifier-only, multi-step, or character-layout support does not imply those capabilities exist here. Native registration remains authoritative for errors it can detect; this UI does not claim to enumerate all other applications' shortcuts.

### Official interaction references

- [Apple: customise keyboard shortcuts](https://support.apple.com/en-gb/guide/mac-help/mchlp2262/mac) edits the existing shortcut by activating its field and pressing the replacement combination. Its guidance also covers conflicts and restoring defaults.
- [Raycast: Command Aliases & Hotkeys](https://manual.raycast.com/command-aliases-and-hotkeys) uses a shortcut recorder with immediate key display and conflict feedback. Vesper likewise saves automatically after capture, with the sidebar pill confirming each write.
- [VS Code: keyboard shortcuts](https://code.visualstudio.com/docs/configure/keybindings) documents pressing the intended combination to define a binding and inspecting conflicting assignments.

These are interaction precedents, not evidence that Vesper implements the products' native key-capture internals or their full key support.

## About and updates studies

The [standalone HTML samples](design/about-updates/index.html) compare three
proposed layouts: A01 Quiet settings, A02 Split view and A03 About sheet. See the
[preview guide](design/about-updates/README.md) for the interactive states and
selection export. These are local review artifacts in the ignored `docs/design/`
directory, consistent with the repository's treatment of exploratory studies.
Settings → About shows the logo, the installed version and one update pill in the
content panel (no tagline, platform, build type or copy button; this is a
macOS-only app, issue #181). Native About menu entries open the
**A03 About sheet**, reusing the same content. Both preserve shared controls and
error recovery. Metadata comes from `get_app_info` independently of model/settings
readiness. A release build is not labeled as a verified stable release channel.

Keep normal-state action wording out of the visible layout: the update state and
its next action live in one pill, not a headline plus a button.

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
[#66](https://github.com/rahulsharmavishwakarma/vesper/issues/66) and
[#65](https://github.com/rahulsharmavishwakarma/vesper/issues/65).

The update pill (`AboutSheet.svelte`, issue #181) is one control for status and
action, using only `Icon.svelte` glyphs and the app's motion tokens. Its states:
`refresh` turning while checking; `check` with a one-time ring flash when current,
swapping on hover to `refresh` and **Check now**; `download` on the accent
**Install x.y.z**; the new `downloading` glyph (the arrow drops into a fixed
tray) with a GPU-scaled progress fill, a soft leading edge and a fixed-width
percentage so the pill never changes size mid-download; `refresh` on the accent
**Restart to update**; `warning` on a danger-tinted **Couldn't check** (or
**Download/Install/Restart failed**) that swaps to **Retry** on hover; and a
static **Updates unavailable** when the build has no updater. A hover swap only
arms after the pointer leaves, so a result that lands under a resting pointer
stays readable instead of reverting to its action. Reduced motion removes the
spin, arrow loop, flash and fills but keeps every label and state.

The earlier manual-download link was removed: it was the only unstyled `<a>`
(raw browser blue and underline) and the in-app updater is the supported path.
Three switch rows sit under the version, each with its `Icon.svelte` glyph (`refresh`,
`download`, `spark`) and the switch on the right using the shared switch skin:
**Check for updates automatically** (default on), **Download and install
automatically** (default off, disabled and dimmed while checking is off) and
**Include beta and pre-release builds** (default off, with a one-line `.note`
beneath it saying that off shows only the latest stable release, on also offers
release candidates that may be less stable; issue #277). They are hidden when the
build has no updater, and the third is also hidden on nightly builds, which follow
their own feed. Installing automatically never restarts the app and reuses
`downloadAndInstall`'s signature check and wait for active work (issue #195).
With checking off, the idle pill becomes a manual **Check for updates** action.

## First-run setup

The one-time setup (issue #270; design study in `docs/design/first-run/`) is
`FirstRun.svelte`, shown by `App.svelte` above the app while the profile's
first-run state is pending (`FIRST_RUN_ENABLED` in `lib/firstRun.ts`). Previews
and tests open it with `?first-run`.

- **Welcome** uses the whole window with one focal point: the bird, "Welcome
  to Vesper", one line and one Get started button. Its single signature motion
  is a thin voice line that gathers into the centre and becomes the bird; the
  words then come into focus in sequence and the bird chirps once. Afterwards
  only the blink remains.
- **Leaving Welcome** places step 2 underneath at once and crossfades Welcome
  away above it, so there is no blank frame. The bird is the one shared
  element: it flies a sampled arc with one ease-in-out into the sidebar logo,
  measured where the logo rests rather than mid-slide. The rail slides in as
  one piece and the content fades in as one.
- **Steps** keep the setup rail (completed steps are buttons for going back;
  later steps are not interactive), **Back** from Voice onward and one
  Continue per step. Going back to Welcome, from Back or the rail, shows the
  composed Welcome screen again (a short fade, no replayed intro); there is no
  empty step 1. There is no Skip. Progress is saved after every step so a quit
  resumes where it stopped.
- **Voice** uses the app's shared controls: the language and microphone
  fields are the same `Select` as Dictate, and the meter is the recording
  waveform (`AudioIndicator`, full reed shape and pitch spacing). The language
  list is every explicit language at least one setup speech model declares in
  `models/languages.json`; automatic detection stays in Dictate and Settings.
  Choosing a language the current speech model cannot hear also selects the
  recommended model for it, so saved settings stay valid, and the hint names
  that model. The microphone prompt appears only from **Allow microphone**; a
  denial names the exact System Settings path and is noticed as soon as access
  is turned on. About 0.6 s of clear input confirms "Heard you", and Continue
  waits for it.
- **Models** shows this Mac as three tiles (chip, memory, free space with a
  bar for this download) that rise in once while the numbers count up. Below,
  the recommended speech model and, under **Tidy my text**, the writing model
  are the app's identity `Select`, the same picker as the Models page. Each
  opens filtered to the language chosen on Voice, best first. Under the
  speech model, accuracy, speed and memory crossfade when the choice changes,
  with one evidence line naming where each number comes from.
- **Picker results** (setup and the Models page, `recommendations.ts`): each
  speech row carries three ring gauges before its download icon: accuracy in
  the chosen language (target), time to write a recording (CPU) and memory
  against this Mac's RAM (amber above a quarter, red above half). Writer rows
  carry instruction following (shield) and a multilingual score (globe).
  Rings draw in when the list opens; a dashed empty ring means no figure
  exists. The row's description starts with a tag such as *Best for English*,
  *Published result*, *Failed Vesper's test · 1 of 32 recordings failed* or
  *Doesn't support Hindi*. Setup locks rows that cannot run here; the Models
  page keeps them selectable so a language can be repaired after a switch.
  Free space must cover the download plus a quarter, and being offline blocks
  the download, each with its reason in the footer. **Download and set up**
  saves the choice, starts the download in the background and moves on; a chip
  at the bottom of the rail shows progress, then **Models ready**, or **Download
  stopped** with **Try again**.
- **Access** shows the microphone and Accessibility. **Open System Settings**
  asks macOS and opens the Accessibility pane, then notices the moment access
  is granted (a second request finishes event posting or clipboard access when
  only those are missing). Granting access turns on typing into apps. After
  eight seconds of waiting it offers **Restart Vesper** (setup resumes here) and
  **Keep text in Vesper**, an explicit outcome that keeps words in the app,
  ready to copy, so nobody is trapped.
- **Shortcut** reuses the Settings recorder for Hold to talk and Press to
  toggle: activate the field, press the keys, Escape cancels.
- **Your words** previews one sample sentence as said and as written. **As I
  said it** and **Polished** are the Formatting-only and fuller cleanup
  presets; with Tidy my text off they are disabled with the reason. Words added
  here are saved to Your vocabulary as words you added. Changing the style
  re-writes the sentence with a soft blur while the bird tilts and chirps;
  adding a word sends it as a chip into its place in the sentence, where the
  bird chirps. Automatic learning is explained, not switched (it is always on).
- **Try it** is locked behind live progress until the models are installed;
  after a restart it checks what is installed and resumes the download. It
  reuses the Dictate recorder's shared classes: the large status heading
  (Ready to try, Listening…, Writing it down…, It works), the **Record**
  button with its ember ring, and the timer above the centred recording
  waveform, with one short hint ("Click to start and stop, or hold to talk")
  that hides while recording; the shortcut is named once, under the card. A
  click starts recording and a second click (**Stop and
  transcribe**) finishes it; holding the button for more than 350 ms works as
  push-to-talk (**Release to finish**). Like Dictate, the step follows the
  recorder status and `transcription-partial` events, so live words stream
  into the practice box itself while speaking (confirmed words in ink, the
  unconfirmed tail in grey, a blinking caret) and become the editable final
  text on stop. Drafts are painted at most once per frame, newest first;
  words that stay the same keep their element (`liveWords.ts` keys them by
  position and text), so only new or revised words ease in and a grey word
  turning final fades to ink, and a capture started with the
  global shortcut shows the same states (its result lands in the box by paste
  or from the latest result). Recording goes through the normal pipeline, and
  **Finish** waits for words to appear.
- **Welcome's bird** calls like the app logo: its two beak dashes pulse every
  2.6 s, the second trailing by 0.35 s, the eye blinks every 4.2 s, and a chirp
  (on its own call, hover or click) plays a stronger burst on top. The flying
  bird keeps the same pulse on its way to the sidebar.
- **Welcome's background** is a fine dot field in the accent colour. Each call
  of the bird (about every five seconds, and on hover or click) sends a soft
  ripple out from its beak; dots near the pointer brighten and drift aside,
  easing after the cursor. Only the bird's body stays clear, so dots never show
  through its eye or wing; there is no glow behind the bird, and hovering it
  shows no button fill.
- **Reduced motion** shows Welcome at once and changes step without the flight.

## Busy and locked controls

The backend runs one operation at a time, so a model download holds the same slot
as recording and transcription, but it does not touch recordings, transcripts or
ordinary settings. The interface keeps the two apart. `TranscriptionPanel`
reports `working` (recording, transcription, removal) and `downloading`
separately: `App.svelte` disables the settings form and History only for
`working`. A download locks only its own model's choices (issue #313):
`asrChoicesLocked` and `writerChoicesLocked` follow the role of the running or
requested download (`downloads.installingKey`, `downloadRequestRole`), so the
writer picker and precision stay usable while the speech model downloads, and
the reverse. The device, removal, Restore defaults and the download/verify
buttons stay locked for any download, and an unknown target (download-all,
automatic audio packages) locks both pickers. A locked control carries a
visible reason (for example, "The speech model and processing device unlock
when this download finishes").
Recording and transcription still wait for a download. The backend mirrors the
split: `save_settings` and `retry_global_shortcut` accept a running download, and
`save_settings` rejects only changes that `AppConfig::conflicts_with_download`
reports for the running download's model (`Runtime::downloading_model`): that
model's own choice (speech model or profile; writer, profile or quantization),
the processing device or the audio frontend. Running downloads alongside
recording is a separate proposal (#314). Do not gate a control on the combined busy flag unless it truly
conflicts with the running operation (issue #273).

## Verification

Exercise the whole task with pointer and keyboard input: initial value, entering edit mode, valid and invalid input, cancellation, focus movement, saving, failure/retry, and reloading. Include slow asynchronous setup and navigation while work is pending. Check narrow and desktop layouts using the rendered component; do not rely solely on a screenshot or a synthetic event pass. Reduced motion must preserve the same state and feedback. Keep real OS shortcut and permission qualification separate from browser mocks.
