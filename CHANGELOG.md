# Changelog

Notable changes to Vesper are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Every release, with its full list of
pull requests, downloads and checksums, is on the
[Releases page](https://github.com/rahulsharmavishwakarma/vesper/releases).
Nightly builds are not listed.

## [Unreleased]

### Changed

- About shows **What's new** in a panel of fixed height that scrolls, instead of
  expanding the page.

## [0.1.2] - 2026-10-29

### Added

- Self-improving dictation that works with every speech model. Vesper learns
  spellings from Text Replacements, your spelling dictionary and the fixes you
  make, and a correction gate repairs near-misses without rewriting ordinary
  words. Learning pauses itself after repeated rejections.
- Rule-based formatting for English numbers, times, dates, emails, links,
  hesitations and self-corrections.
- Opt-in automatic workspaces, and opt-in learning from the app you dictate
  into.
- History shows time saved, time recorded, words and recordings.

### Changed

- About lists **What's new** under the Install button, grouped by kind, from this
  changelog instead of a raw pull-request list.
- Settings has one tab per task: Dictation, Writing, Vocabulary, Projects,
  Appearance, Privacy and About, with grouped rows in the macOS style.
- Speech model recommendations come from Vesper's own per-language results.
- Update checks query the stable and pre-release feeds at the same time, time out
  after 20 seconds, and read the stable feed from GitHub Pages first.

### Fixed

- Data saved by a newer version no longer stops an older one: unknown settings
  and values are skipped, History skips a record it cannot read, and unreadable
  learned words never block recording. Long errors fit the capture panel.

### Removed

- The folder opt-in for learning words. A stored folder path is erased on
  upgrade.
- VibeVoice benchmark tooling. Vesper does not support VibeVoice models.

### Security

- Patched `zstd-sys`, `msgpack`, `filelock`, `setuptools` and `tokio` advisories.
  The remaining `zstd` advisories affect only a build-time dependency of the speech
  runtime and are documented in [CI and releases](docs/ci-and-releases.md).

## [0.1.1] - 2026-10-22

### Added

- Speech models: Moonshine Base (English), Granite Speech 5.0 TurboCTC,
  Phonon-2 (English) and Parakeet Redux (25 languages). Granite, Phonon-2 and
  Redux run on the Apple GPU.
- Parakeet v3 as one GGUF file for CPU and GPU, and its encoder on the Apple
  Neural Engine. Automatic picks the engine.
- A guided first-run setup with model recommendations for the Mac.
- An opt-in switch in About for beta and pre-release updates.
- A branded installer window when the DMG opens.

### Changed

- Parakeet v3 decodes 15–27% faster with identical output, and its MLX weights
  download at half the size.
- Parakeet v3 CPU downloads use 4-bit weight-only files.
- The writing model's precision is chosen automatically.
- Downloading a model locks only that model, not the rest of the app.

### Fixed

- Downloads are no longer reported as damaged; the whole app is signed ad hoc.
- When macOS refuses the GPU library, Vesper reports why and falls back to CPU.
- Model rows stay steady while verifying, without a loading flash at startup.

## [0.1.0] - 2026-10-20

First stable release.

### Added

- Automatic app updates from a signed update feed.
- A redesigned About page with update status.

### Changed

- Multichannel audio mixes down to mono while keeping one live channel at full
  level.
- Rust audio and inference dependencies updated.

### Known limitations

- The app is not notarized with an Apple Developer ID, so macOS warns on first
  launch ([#64](https://github.com/rahulsharmavishwakarma/vesper/issues/64)).

[Unreleased]: https://github.com/rahulsharmavishwakarma/vesper/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/rahulsharmavishwakarma/vesper/releases/tag/v0.1.2
[0.1.1]: https://github.com/rahulsharmavishwakarma/vesper/releases/tag/v0.1.1
[0.1.0]: https://github.com/rahulsharmavishwakarma/vesper/releases/tag/v0.1.0
