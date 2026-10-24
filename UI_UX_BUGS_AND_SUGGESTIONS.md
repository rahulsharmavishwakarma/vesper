# Vesper UI/UX Testing Findings & Suggestions

After conducting automated Playwright testing and visual inspection of the Vesper Desktop UI, I have identified a few UI/UX bugs and areas for potential improvement, specifically related to responsiveness, accessibility, and interaction design.

## 1. UI Responsiveness Issues (Mobile / Narrow Window View)
- **Bug**: When resizing the application to a narrow width (e.g., `400px` width), certain interactive elements become difficult or impossible to interact with. For instance, in automated tests on a `400x800` viewport, the "Record" button on the Dictate page became inaccessible or overlapped, leading to timeouts.
- **Suggestion**: Implement a strict minimum width for the application window in Tauri settings to prevent the UI from squishing too much, OR improve the responsive flex/grid layouts on the Dictate screen so that primary actions (Record, Stop, Cancel) remain easily clickable and fixed at the bottom/top even in constrained views.

## 2. Accessibility & Semantic Navigations
- **Bug / Flaw**: Tests targeting navigation elements in the Settings page (e.g., clicking "Shortcuts" or "Writing" tabs) initially struggled because the locators resolved to multiple non-interactive elements (like `<h2 id="...">`) instead of clear, semantic navigation tabs.
- **Suggestion**: Ensure all navigation tabs use semantic `<button>` or `<a>` tags with appropriate `role="tab"`, `aria-controls`, and `aria-selected` attributes. This will heavily improve screen reader accessibility and make automated testing significantly more robust.

## 3. Visual Feedback & Contrast
- **Observation**: The Ember dark theme looks fantastic, but some secondary text (e.g., "Choose a language supported by the speech model and, when enabled, the writing model") uses a muted gray that may sit borderline on the WCAG AA contrast ratio against the dark background.
- **Suggestion**: Verify the contrast ratio of the `--color-muted` text token. A slight bump in lightness can greatly improve readability for users with mild visual impairments without ruining the dark theme aesthetic.
- **Observation**: During keyboard navigation, focus rings (outlines) might not be distinctly visible on all interactive elements like the model select dropdowns or tab navigations.
- **Suggestion**: Ensure a highly visible focus state (using the primary ember/orange brand color) is applied globally for keyboard-focused states (`:focus-visible`).

## 4. First-Run Setup & Animation States
- **Observation**: The guided setup flow has smooth animations, but during transitions, buttons can become temporarily unclickable or their DOM nodes re-rendered in ways that block rapid user input.
- **Suggestion**: Make sure transition animations (`reducedMotion` aside) do not detach event listeners early or block the main thread. While `reducedMotion: reduce` works fine per `AGENTS.md`, standard motion should also guarantee fast interaction times for power users who click through quickly.

## 5. Layout Consistency
- **Observation**: The alignment of the "Cancel operation" button next to "Transcribe audio file" in the recording active state seems slightly disjointed compared to the central alignment of the main "Stop and transcribe" button.
- **Suggestion**: Group secondary actions (Cancel, Upload) symmetrically under or around the primary recording button to keep the central focal point balanced.
