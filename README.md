<p align="center">
  <img src="docs/assets/vesper-banner.svg" alt="Vesper" width="1200">
</p>

<h1 align="center">Voice to text, on your device.</h1>

<p align="center">Local models · Early&nbsp;development</p>

<p align="center">
  <a href="https://github.com/rahulsharmavishwakarma/vesper/releases/latest"><img alt="Download the latest stable release" src="https://img.shields.io/badge/Download-latest%20stable-ff6b3d?style=for-the-badge"></a>
</p>

<p align="center">
  <a href="https://github.com/rahulsharmavishwakarma/vesper/releases/latest"><img alt="Latest stable release" src="https://img.shields.io/github/v/release/rahulsharmavishwakarma/vesper?label=stable&color=ff6b3d"></a>
  <a href="https://github.com/rahulsharmavishwakarma/vesper/releases"><img alt="Latest release including prereleases" src="https://img.shields.io/github/v/release/rahulsharmavishwakarma/vesper?include_prereleases&label=latest&color=lightgrey"></a>
  <a href="https://github.com/rahulsharmavishwakarma/vesper/actions/workflows/check.yml"><img alt="Checks on main" src="https://img.shields.io/github/actions/workflow/status/rahulsharmavishwakarma/vesper/check.yml?branch=main&label=checks"></a>
  <a href="LICENSE"><img alt="License: Apache 2.0" src="https://img.shields.io/github/license/rahulsharmavishwakarma/vesper"></a>
</p>

<p align="center">
  <a href="#download">Download</a> ·
  <a href="#getting-started">Get&nbsp;started</a> ·
  <a href="docs/user-guide.md">User&nbsp;guide</a> ·
  <a href="#development">Build&nbsp;from&nbsp;source</a> ·
  <a href="https://github.com/rahulsharmavishwakarma/vesper/issues">Issues</a>
</p>

Vesper turns microphone recordings and audio files into text using local models.
Dictate with a keyboard shortcut, review your words, and optionally clean up
punctuation and formatting. No account or cloud API key is required.

## Features

- **Quick dictation.** Hold to talk or toggle recording with a global shortcut and floating controls.
- **Audio-file transcription.** Import a recording and review its transcript alongside microphone recordings.
- **Optional text cleanup.** Improve punctuation and formatting while keeping the original transcript available.
- **Personal vocabulary.** Remember spellings across recordings and model changes, select project vocabulary with app rules, and review eligible TextEdit corrections. See [scope and limits](docs/personalization/README.md).
- **Local history.** Search, review, copy and export saved recordings and text.
- **Your choice of models.** Select speech and writing models independently. Current speech adapters show live drafts, with update speed and language support depending on the model and device.
- **Automatic acceleration.** Compatible models use the Mac GPU with matching model files chosen during download. See [supported combinations](docs/accelerators.md).

See the [user guide](docs/user-guide.md) for daily use and the
[language compatibility guide](docs/language-support-and-recommendations.md) for model-specific limits.

## Download

Vesper is available as a macOS DMG for Apple-silicon Macs. The badges above show
the latest stable version and the latest release including prereleases.

<p align="center">
  <a href="https://github.com/rahulsharmavishwakarma/vesper/releases/latest/download/vesper-macos-arm64.dmg"><img alt="Download the latest Mac DMG" src="https://img.shields.io/badge/Download-latest%20Mac%20DMG-ff6b3d?style=for-the-badge&logo=apple&logoColor=white"></a>
</p>

[Download the latest Mac DMG](https://github.com/rahulsharmavishwakarma/vesper/releases/latest/download/vesper-macos-arm64.dmg).
Versioned downloads and their `SHA256SUMS` are on the
[Releases page](https://github.com/rahulsharmavishwakarma/vesper/releases).
Intel Macs, Windows and Linux are not current release targets.

The app is **not yet notarized** with an Apple Developer ID, so macOS warns on
the first launch. Drag Vesper to Applications and open it; if macOS says it cannot
verify the app, go to **System Settings → Privacy & Security** and choose **Open
Anyway**. If it instead says **"Vesper" is damaged and can't be opened** (the
0.1.0 download, which is only partly signed), run
`xattr -cr /Applications/Vesper.app` in Terminal and open it again. Later releases
seal the whole app, so they show the normal warning. Verify the download against
`SHA256SUMS` if you want to confirm it.
Signing and notarization are tracked in
[issue #64](https://github.com/rahulsharmavishwakarma/vesper/issues/64).

You can also [run from source](#development). Packaging details are in the
[release guide](docs/ci-and-releases.md#release-workflow-and-download-url).

## Getting started

The first launch opens a short guided setup: choose your language and check the
microphone, download the recommended models for this Mac, allow Accessibility so
Vesper can type into other apps, confirm your shortcut, add words Vesper should
know, and try a first dictation. It appears once; see
[first launch](docs/user-guide.md#first-launch).

Afterwards, in **Dictate**:

1. Press **Record** (or hold your shortcut in any app), speak, then **Stop and transcribe**. You can also import an audio file.
2. Review the result and copy the text. Saved recordings appear in **History**.
3. Change models in **Models** and preferences in **Settings** at any time. Supporting audio models are set up automatically; see [audio setup](docs/user-guide.md#experimental-audio-processing).

Default shortcuts while the app is running:

| Action | Shortcut |
| --- | --- |
| Hold to talk | `⌥ Shift D` |
| Start / stop recording | `⌘ Shift Space` |

Change these in **Settings → Dictation**. Pasting into the original text
field is optional and requires macOS Accessibility permission; Copy remains
available. See [quick capture and permissions](docs/user-guide.md#quick-capture).

## Privacy

Speech recognition and text cleanup run locally on your device. Audio and
transcripts are not uploaded for processing. Internet access is needed to
download models and runtime components; installed models can then work offline.

Recording history is saved locally by default. Turn off **Save recordings and
transcripts** in Settings to stop archiving new sessions. Personal vocabulary
and compact correction evidence are stored separately on the device. Read the
[storage and privacy details](docs/user-guide.md#privacy-and-storage).

## Development

The app uses Rust, Tauri, Svelte and TypeScript. Install the
[macOS prerequisites](docs/development.md#prerequisites), then:

```sh
git clone https://github.com/rahulsharmavishwakarma/vesper.git
cd vesper
make setup
make dev
```

Model downloads happen separately in the app. The native app does not require Python;
contributor checks use Python 3.12 or newer.

The GitHub repository is named `vesper`; a local checkout directory may have any name.
The app and project-owned packages use Vesper; see
[product naming and compatibility](docs/development.md#product-name-and-compatibility).

```sh
make setup-checks
make hooks
make check
```

For builds, formatting, tests and release commands, see the
[development guide](docs/development.md) and [CI documentation](docs/ci-and-releases.md).
See the [architecture](docs/architecture.md), [inference backend architecture](docs/inference-architecture.md) and [research](docs/research.md)
guides for technical background.

## Contributing and support

Read [Contributing](CONTRIBUTING.md) before starting work, search
[existing issues](https://github.com/rahulsharmavishwakarma/vesper/issues), and
follow the [Code of Conduct](CODE_OF_CONDUCT.md). Features, fixes and maintenance
include updates to the relevant documentation.

For help, see [Support](SUPPORT.md). Report vulnerabilities privately using the
[security policy](SECURITY.md). Keep private recordings and transcripts out of issues.

## License

Application code is licensed under the [Apache License 2.0](LICENSE),
Copyright 2026 Rahul Sharma. Model, runtime, font and dependency licenses
remain separate; see [licenses and model terms](docs/licenses-and-model-terms.md)
and [model provenance](models/README.md).
