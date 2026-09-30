# Beta qualification

**Release:** 0.2.0-beta.4 · **Application:** 0.2.0 · **Build:** 84 · **Architecture:** arm64

The app declares macOS 14 or later; this qualification used Apple silicon on
macOS 27.0. Bundled Core ML semantic ranking requires macOS 15 or later. Other
OS/hardware combinations are not claimed tested.

## Build and regression evidence

- Compatibility suite: 1,176 passed, 29 opt-in tests skipped, zero failures.
- Rust suite: 146 passed, two ignored. Focused Swift source/metadata checks: 59
  passed; 15 execution checks passed after the final cleanup adjustment.
- A real bundled model test confirmed 16 test originals in 0.218 seconds, opened
  originals before EXIF completion, collected GPS during tagging, and passed
  location-search checks. The fixture originals remained unchanged.
- All native executable bytes and executable entitlements in this release match
  that tested build after removal of code signatures. Release changes are bundle
  version/updater metadata, clean build provenance and normalized notice modes.
- All 418 distributable payload entries are classified with no unresolved audit
  blockers. Read-only DMG inspection matched the exact signed candidate.
- The optional-diagnostics intake service matches the reviewed and tested service
  already deployed. Ten service checks previously passed; its public and SSH-only
  review processes were verified active for this release.

## Signed update and saved data

A real Sparkle trial installed the exact beta-4 candidate over an isolated beta-3
build 76 copy. The old copy used a private updater preferences domain. All 21
fixture-library files stayed unchanged during installation. Reopening the new
worker preserved all 17 authoritative source tables, including accepted details,
People edits and review history. Fixture originals were unchanged. Customer data
was not a trial target.

The trial used loopback-only HTTP with real Ed25519-signed feed and archive and
an external updater harness. Production UI save/drain gates have separate tests.
Public HTTPS artifact verification is performed during publication. Beta 1's
prior negative update tests and compatible manual rollback are reused; they were
not all repeated. Authorization denial, wrong bundle identity and interruption
during final replacement remain unqualified. No automatic downgrade is supported.

## Limits

Location labels are approximate GeoNames matches, not verified street addresses.
Search needs saved GPS/capture evidence and respects metadata preferences.
A matching filename or selected root alone does not establish original identity;
explicit confirmation may still be required. Cloud files can remain unavailable.
Small-fixture checks do not establish broad model accuracy or performance on every
library and cloud provider.

This beta is ad-hoc signed, not Apple Developer ID signed or notarized. A fresh
quarantined first launch under a new macOS user remains unqualified. Signatures
authenticate the downloaded bytes, not their safety. Signing-key backup is owner
reported; independent recovery has not been tested.

There is no OS-enforced network/filesystem sandbox or absolute no-internet
guarantee. Explicit downloads, updates, links and voluntary feedback use the
network. No app advertising analytics or telemetry is added.

GitHub Actions could not start because of account billing/spending limits; the
checks above ran locally. No new completed Codex Security scan is claimed for
this release. App-source review belongs in the private development repository.
This public repository and GitHub's automatic source archives contain distribution
documentation and verification tooling, not application source.
