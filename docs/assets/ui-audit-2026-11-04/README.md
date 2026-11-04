# UI audit screenshots — 4 November 2026

Evidence for the animation, button and flow audit tracked in
[#395](https://github.com/rahulsharmavishwakarma/vesper/issues/395), with findings
[#393](https://github.com/rahulsharmavishwakarma/vesper/issues/393) and
[#394](https://github.com/rahulsharmavishwakarma/vesper/issues/394).

Captured in Chromium against the browser preview (`make web`) and the synthetic
fixture (`apps/desktop/tests/interaction-preview.html`, mocked IPC, synthetic
data only) on `main` at `aec024c`. No private audio, transcripts or user data.

| File                           | What it shows                                                          |
| ------------------------------ | ---------------------------------------------------------------------- |
| `fixture-dictate.png`          | Dictate, idle, Dark                                                    |
| `fixture-recording.png`        | Dictate while recording, with live transcript draft                    |
| `select-open.png`              | Language Select open, selected option highlighted                      |
| `fixture-history-expanded.png` | History with a recording expanded                                      |
| `fixture-history-nomatch.png`  | History search with no matches (doubled empty state, #383)             |
| `history-empty-day.png`        | History filtered to a zero-recording day                               |
| `fixture-models.png`           | Models, Local engine before any reading (#379)                         |
| `fixture-appearance.png`       | Settings → Appearance with capture-design preview                      |
| `settings-writing.png`         | Settings → Writing, shortcut fields (#380)                             |
| `light-dictate.png`            | Dictate, Light theme                                                   |
| `fr-welcome.png`               | First-run Welcome mid-intro, bird only (#378)                          |
| `fr-step5.png`                 | First-run “Make it yours”                                              |
| `fr-step6.png`                 | First-run “Say something” practice step                                |
| `fixture-360.png`              | Dictate at 360 px wide                                                 |
| `states.png`                   | Eight-state control specimen (`?states`)                               |
| `removal-dialog.png`           | Model removal confirmation dialog                                      |
| `native-recording.png`         | Native `make dev` window while recording (real microphone, live draft) |
| `native-settings.png`          | Native Settings → Dictation with icon rail                             |
