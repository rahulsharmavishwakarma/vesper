# ADR-0001: Local-first native desktop pipeline

- Status: Accepted
- Date: 2026-09-14

## Context

Dictation contains private audio and text. The application also needs microphone access, native inference acceleration, global shortcuts, durable original audio, and predictable offline behavior. A browser-only or cloud-required design would weaken those properties and add a service dependency.

## Decision

Build the product as a Tauri desktop application with a Svelte UI and Rust-owned native lifecycle. Run speech recognition and cleanup on the user's device. Python is allowed for future offline research/training tooling but is not part of the shipped inference path. Network access during normal use is limited to an explicit user-requested model installation.

## Consequences

- Audio and transcript data remain local unless the user exports or moves them.
- Platform packaging, native toolchains, microphone permissions, and accelerator validation are product responsibilities.
- Model files are larger than the application bundle and are installed separately.
- Cloud-only model APIs are research candidates, not local runtime support.
- Cross-device sync, cloud backup, and server-side inference are out of scope until a new ADR changes the privacy boundary.
