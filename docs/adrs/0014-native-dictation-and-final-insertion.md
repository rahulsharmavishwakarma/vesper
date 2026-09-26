# ADR-0014: Native hold/toggle dictation and final text insertion

- Status: Accepted
- Date: 2026-09-16
- Approval and tracking: [GitHub #4](https://github.com/rahulsharmavishwakarma/transcribe/issues/4)

## Context

Quick capture previously depended on a WebView toggle handler and offered the finished result inside the main app. Hold-to-talk needs to handle key release during startup and continue working independently of WebView readiness. Automatic insertion needs to remember the destination before the recorder appears and keep it associated with the same operation through finalization.

The approved scope is a macOS-first native dictation controller and platform integration. Existing audio capture, inference ownership, native Nemotron streaming, model selection, writing, persistence, and the no-general-time-quota policy remain authoritative. This is application integration, not a new inference method. No paper result or competitor latency is presented as a local measurement.

## Decision

A Rust controller owns shortcut state, generation, trigger mode, cancellation intent, and delivery status. The existing runtime owns audio/inference and its operation ID. Press repeat is ignored. Hold release during startup records a Stop request for that same session; cancellation suppresses insertion synchronously. Native cancellation checks both the session generation and the runtime operation ID, so a delayed cancel cannot affect later work. Toggle and menu/button capture use the same controller. Escape is registered only during a native dictation session.

The UI consumes a retained native status snapshot and revisioned events. It shows Listening only after microphone startup is acknowledged, renders existing operation-scoped partials, and rejects older session/revision snapshots. Non-streaming adapters retain completed-recording behavior. Final delivery runs before the runtime is released for another operation. Main-window and imported-file recordings retain their explicit in-app output; shortcut/menu/floating-bar recordings use the saved output preference.

Settings retain legacy `recordingMode` and `globalShortcut` for migration and add nullable `holdShortcut`/`toggleShortcut` overrides, with both actions active simultaneously. Old configurations preserve the selected shortcut for its previous action and choose an unused default for the second action. `dictationOutput` (`app`/`paste`) retains its saved value and defaults to Keep in app. Choosing paste requests system access explicitly; passive permission checks never prompt. macOS pasteboard access policy is respected before automated preservation reads, preventing a first-use clipboard prompt from stealing the destination's focus during delivery.

Each recording action accepts a custom modifier/key combination through a direct shortcut field and the existing settings-save path. Clicking the field records the actual key press into the draft; Save changes activates it. A temporary editor session routes already-registered combinations from the existing native callback to the main WebView instead of starting dictation. Other supported combinations use DOM keyboard events. Session identity, focus-loss cleanup, and consumed repeat/release events isolate editing from dictation and newer fields. Core/frontend validation preserves the supported key contract and rejects platform-equivalent Hold/Toggle combinations. Native registration canonicalizes both pairs through the pinned `global-hotkey` 0.8.0 parser before transactional replacement. This focused extension adds no keyboard hook, dependency, inference path, or insertion behavior; see [the shortcut configuration guide](../development.md#separate-hold-and-toggle-shortcuts).

### Native presentation and isolation

`vesper-desktop-platform` is the narrow unsafe boundary, following the existing native-runtime boundary pattern. It uses a small Objective-C implementation compiled with the existing `cc` build dependency and Apple's public AppKit, Accessibility, CoreGraphics, and Carbon APIs. The remaining Rust workspace retains its unsafe-code prohibition.

Allocate a real nonactivating NSPanel on the main thread. Retain the hidden original Tao NSWindow and its complete WebView content view; install a non-null placeholder on the original window and host the retained content in the panel. Keep the original window/delegate allocation and class unchanged. Native panel functions own show/hide/position, while the retained WebView keeps Tauri IPC. Panel teardown restores the content before releasing the native host. The UI does not call the old window's show/hide/geometry/drag APIs. This uses public view-hosting APIs but is not an upstream Tauri arbitrary-window reparenting contract; native lifecycle validation must accompany upgrades to Tao/Wry/Tauri.

The panel cannot become key/main and is positioned above the Dock area of the display under the pointer. It preserves the approved visual style. Permission checks, AX references, clipboard data, and panel objects remain native/main-thread-owned; external field contents are not sent to the WebView or persisted.

### Delivery contract

Before presenting the panel, retain the frontmost PID, accessible editable element, and selected range. Before delivery, check those again, along with enabled/editable state, secure-field/secure-input exclusions, permission state, and held modifiers. Unavailable attributes, changed targets, and unqualified terminal destinations use Copy fallback. No whole-field Accessibility value replacement is performed.

For an eligible target, preserve available pasteboard representations in bounded memory, place the final text on the clipboard, and dispatch Command+V once. Resolve V through the active keyboard layout rather than assuming a physical key. Consume the target before dispatch; never automatically retry uncertain delivery. No Return/submit event is synthesized. The UI reports **Paste sent**, because posting events does not prove the destination consumed the text.

Restore the clipboard after one second only if its ownership counter still matches the app's temporary write. Preserve newer copies and restore pending owned content during orderly shutdown. A second paste waits for outstanding owned restoration. Deferred clipboard formats, access restrictions, slow paste consumers, and cross-process timing prevent a universal preservation/delivery guarantee; use Copy on preflight failure. Explicit Copy remains available in the panel and History even after paste dispatch.

### Electron accessibility readiness (approved 24 September 2026)

A VS Code integrated-terminal report exposed a separate failure from an earlier
saved **Keep in app** setting: Electron did not expose a focused editable field
when the shortcut began. TextEdit success did not qualify this destination.
Electron documents `AXManualAccessibility` for assistive clients. The inspected
Electron 42.10.0 implementation (VS Code 1.138.0, Chromium 148.0.7778.280) enables
the complete tree after a two-second debounce; repeated true requests reset it.
Writing the attribute successfully does not establish readiness, and its getter's
exact mode comparison is not used as a readiness test.

The approved extension registers an AppKit `NSWorkspace` activation observer
only while the persisted output preference is `paste`. It also prepares the
current app when enabled. After checking existing Accessibility permission,
the native adapter requests the documented attribute only if it is settable,
with bounded messaging timeouts. Successful requests are retained once per live
process, including across preference toggles; terminated processes are pruned.
The cache is capped at 64 live entries and skips additional requests while full.
Failed requests can be retried on a later activation. No polling loop, sleep,
new keyboard hook, field-text read or inference-lifetime change is introduced.

Startup and successful settings save/reset apply the persisted preference on
the main thread. Each queued callback reads the latest saved setting so stale
callbacks cannot re-enable preparation. Disabling removes the observer;
shutdown also clears its cache. Neither sends a false accessibility request to
another app, since that could disrupt another assistive client.

Capture makes one guarded preparation attempt if a previous activation happened
before permission was granted. It still captures the original field and selection
immediately, before showing the panel. An unready provider uses Copy fallback for
that recording; the adapter never waits and adopts a later field. A shortcut
pressed immediately after a fresh editor launch can therefore still fall back.
Target identity, selection, secure-field, operation, cancellation and clipboard
checks are unchanged. Live Nemotron preview remains in the recorder, followed by
one final paste; live external-field insertion below is still a separate proposal.

## Consequences and validation

- Hold, Toggle, and native presentation work through the existing inference path. Cold model startup and optional writing still contribute to latency; initial readiness is visible rather than implied.
- `captureReadyMs` measures native request to startup acknowledgment; `stopToFinalMs` measures Hold release/Toggle Stop request to completed pipeline output; `finalToDispatchMs` is populated only for dispatched paste. These are observation fields, not measured performance claims. Existing streaming event/stage diagnostics remain separate.
- macOS Accessibility/event posting and, where required, clipboard access must be enabled. Missing permission preserves the recording result for Copy.
- Cross-application focus/paste is not an atomic OS transaction. App/provider behavior, fullscreen Spaces, multiple displays, and different keyboard layouts require native qualification. Windows/Linux retain recording and manual Copy; automatic insertion is unavailable there in this delivery.
- Hybrid/bare-Fn shortcuts, wake phrases, automatic endpointing, live partial insertion, and auto-send remain separate scope.
- Rust tests cover Hold/Toggle/repeat/startup release/cancellation/session isolation; browser fixtures cover revision ordering, permissions/preferences, real partial rendering, Cancel, Copy, and fallback. `sh scripts/test-dictation-native.sh` checks actual NSPanel/WebKit hosting and teardown, focus retention on show, and multi-format/ownership-safe restoration using a private pasteboard. It never opens a microphone, prompts for permission, or injects text into another application. The separate `dictation_overlay_smoke` desktop example verifies actual Tauri IPC after panel hosting and repeated native show/hide.
- The native smoke also tests the opt-in accessibility listener using intercepted AX requests: grant recovery, unsupported and transient failures, repeated activation, preference toggles, process replacement, cache limits, main-thread guards and teardown. It does not modify real applications' accessibility state.
- Separate native adapter checks on 20 September 2026 verified exact caret insertion and selected-text replacement in disposable TextEdit documents, including Hindi, emoji and multiline text. An isolated VS Code terminal with its accessibility provider already prepared received the exact payload inside bracketed-paste markers (LF normalized to CR), with no additional Return. Duplicate dispatch was rejected. Those checks did not use the microphone or physical shortcut; correction observation and end-to-end latency remain separate qualification gates in #4/#7/#61.
- On 24 September 2026, an isolated VS Code profile with a non-executing raw-byte terminal receiver also exercised the implemented activation listener. Resetting that fixture's provider made initial capture fail; activating it with preparation enabled made capture succeed. Both cold-provider and already-ready runs received the exact Hindi/emoji/multiline payload with bracketed paste, LF-to-CR normalization and no extra submit. Cancelled targets, a switch to the test harness's own window, and duplicate dispatch were rejected; refusal left clipboard ownership unchanged. Listener removal was verified. This is actual native adapter delivery, not a physical-hotkey/microphone end-to-end test or qualification of every Electron field.

## Sources

- [Tauri press/release shortcuts](https://v2.tauri.app/plugin/global-shortcut/)
- [Apple NSPanel](https://developer.apple.com/documentation/appkit/nspanel), [nonactivating style](https://developer.apple.com/documentation/appkit/nswindow/stylemask-swift.struct/nonactivatingpanel), [content view hosting](https://developer.apple.com/documentation/appkit/nswindow/contentview)
- [Accessibility trust](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions), [AXUIElement](https://developer.apple.com/documentation/applicationservices/axuielement)
- [Electron accessibility integration](https://www.electronjs.org/docs/latest/tutorial/accessibility), [Electron 42.10.0 provider implementation](https://github.com/electron/electron/blob/v42.10.0/shell/browser/mac/electron_application.mm)
- [Pasteboard ownership](https://developer.apple.com/documentation/appkit/nspasteboard/changecount), [pasteboard access policy](https://developer.apple.com/documentation/appkit/nspasteboard/accessbehavior)
- [VoiceInk interaction modes](https://tryvoiceink.com/docs/shortcuts), [documented clipboard delivery limitations](https://tryvoiceink.com/docs/clipboard-issues)

Implementation inspection used Tauri 2.11.5, tauri-runtime-wry 2.11.4, Tao 0.35.3, Wry 0.55.1, and the installed Apple SDK. ASR/runtime pins and capability evidence remain in ADR-0006/0012 and the canonical inference research map; this decision changes none of them.
## Proposal: live text in the original field — approval pending

Problem: the approved ADR-0014 path shows Nemotron partials in the recorder and inserts final text once. The user now requests visible words inside the focused external field during recording, with no Copy click in the normal path.

### Concrete scope

1. Add an explicit live-insertion output choice in Settings, using the existing native dictation controller and platform crate. Both independently configured Hold and Toggle shortcuts use this preference.
2. For the existing native-streaming Nemotron adapter, consume operation/sequence-scoped partials in the native controller; coalesce superseded updates and allow at most one main-thread destination update at a time. Do not feed mutable partials into the writing model. This is a frozen-weight application integration, not decoder/kernel work or checkpoint/training changes.
3. Capture the original app, editable field, selection, and bounded text guards. Require actual read/range/selected-text write support. Replace only the span inserted by this dictation, verify text and caret after each write, use UTF-16 range accounting, and keep destination context native/ephemeral. No whole-field rewrite, clipboard per partial, or submit keystroke.
4. Release Hold or press Toggle again to finalize once. Apply the existing final writing result to the same verified span if the user has not changed it.
5. Stop external updates if focus, caret, text ownership, permissions, or provider behavior changes. After any inline write, never automatically paste the full transcript as fallback. Keep the transcript available in the recorder/History. Cancel restores the initial selection only when exact ownership can still be verified.
6. If inline capability is unavailable before any write, use the existing final auto-paste. Completed-recording adapters retain final delivery; this proposal does not silently switch models or claim real streaming for Cohere/Qwen/LFM adapters.

### Affected components and tradeoffs

Extend dictation.rs, the existing streaming update callback, vesper-desktop-platform, output settings/recorder state, and focused tests/docs. No new service, input source, inference owner, runtime, or model download is proposed. macOS first; other platforms keep existing behavior.

Accessibility calls are separate cross-process operations, with no atomic compare-and-replace or universal undo/composition guarantee. Per-provider capability checks and verification reduce risk but cannot promise every editor. Initial qualification: synthetic AppKit fields, TextEdit, Safari/Chrome ordinary input/textarea, then additional editors explicitly. Unqualified or incompatible destinations use final delivery when no inline write occurred.

Cold streaming startup currently loads the recognizer before opening the microphone. This proposal preserves that ownership/admission order and makes readiness explicit; it does not claim to capture speech before microphone readiness. A capture-first buffering or prewarming change would need separately scoped design and measurements.

### Evidence

- Product behavior: Superwhisper documents recorder-window realtime captions and final cursor paste separately: https://superwhisper.com/docs/common-issues/realtime and https://superwhisper.com/docs/get-started/settings-advanced. Apple Dictation supports insertion-point dictation: https://support.apple.com/en-mide/guide/mac-help/mh40584/mac. These are product capabilities, not local measurements or proof of their internals.
- Public API: Apple setAccessibilitySelectedText(_:), setAccessibilitySelectedTextRange(_:), AXUIElementIsAttributeSettable, AXUIElementSetAttributeValue. Older AX C headers annotate selected text read-only; modern AppKit and official provider implementations expose setters. Probe actual attributes rather than assuming support.
- Official provider source: WebKit d843c238e992864a057149c5a14c0c191629b4b6, WebAccessibilityObjectWrapperMac.mm, and Chromium c5551b29901fdbceebe1eb55ac2ada8b51982da6, ax_platform_node_cocoa.mm implement selected-range/text replacement. Provider source is not proof every installed browser/editor accepts it.
- Existing inference pins remain ADR-0012: Nemotron checkpoint ea30d66debe3740a08b573244286791d423d6b3e; NeMo-Speech.cpp 4f9676226f667d14608487df744f375db87127f8. No published WER/latency gain is claimed for inline delivery. The canonical research map remains docs/research-streaming-inference-papers.md.
- Local measurement: no live external-field insertion measurement yet. Feasibility in each destination and user-visible latency are unverified until native qualification.

### Validation

Test provisional revisions, UTF-16 Hindi/emoji/combining marks, selected-text replacement, unchanged surrounding text, user typing/caret/focus changes, app closure, permission loss, timeouts after writes, cancellation, long dictation, slow final writing, and undo/redo. Verify no clipboard mutation during partials, no duplicate final paste, and no send action. Measure shortcut-to-capture-ready, first partial, first external visible text, release-to-final and final-field update separately, recording exact model/runtime/hardware/backend/precision state per the canonical benchmark instructions.

Approval: pending explicit user confirmation of this extension. Separate-shortcut fixes can proceed under the current direct request.
