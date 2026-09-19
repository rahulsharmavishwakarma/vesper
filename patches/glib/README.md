# GLib 0.18.5 compatibility backport

Tauri 2's Linux GTK3 graph requires GLib 0.18. `vendor/glib` contains the
unmodified crates.io 0.18.5 archive except for the two-line `VariantStrIter`
mutable out-pointer fix from upstream
[`05dff0e`](https://github.com/gtk-rs/gtk-rs-core/commit/05dff0ee696f9bcd8617cd48c4b812d046d440cb).
The original MIT license, manifests and source notices are retained.

`provenance.json` records the official archive checksum and every resulting
file checksum. The workspace uses a local Cargo patch; no version is relabeled.
`make audit-rust` verifies the complete source tree, workspace patch, lockfile
and actual Cargo resolution before running the strict RustSec audit. Missing/changed/extra source files, symlinks, registry copies,
duplicate GLib packages and resolution outside this source all fail the check.
Cargo-audit does not match crates.io advisories to local path packages. After
verification, the wrapper creates a temporary audit-only lockfile restoring
GLib's original crates.io identity and checksum. It runs `cargo audit --deny
unsound` against that file with exactly one approved exception:
`RUSTSEC-2024-0429`, repaired by this verified backport. The actual build lockfile
is untouched. All other GLib advisories, vulnerabilities and soundness failures
remain fatal. A synthetic second GLib advisory tests that behavior with the real
audit client. This is a maintained source fix, not an upstream release.

`make test-glib-backport` exercises forward/reverse/skipping/empty iterators in
an optimized build. It needs system GLib development files and pkg-config; CI
installs them on Linux. `tests/glib-backport/Cargo.lock` pins that regression
environment; the source guard also requires its registry dependencies to appear
in the audited workspace graph. Neither GLib nor this test is added to the macOS
application graph.

To update the backport, start with the checksum-verified official archive,
review the upstream security fix, regenerate the complete file manifest and
run the optimized test plus audit guard tests. Remove the local
patch when Tauri's dependency graph accepts an upstream version recognized as
fixed by RustSec. Do not edit the vendor source with project formatters.
