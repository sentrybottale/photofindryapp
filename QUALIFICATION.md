# Beta qualification

**Release:** 0.2.0-beta.6 · **Application:** 0.2.0 · **Build:** 86 · **Architecture:** arm64

The app declares macOS 14 or later; this qualification used Apple silicon on
macOS 27.0. Bundled Core ML semantic ranking requires macOS 15 or later. Other
OS/hardware combinations are not claimed tested.

## Build and regression evidence

- Compatibility suite: 1,177 passed, 29 opt-in tests skipped, zero failures.
- Native-mode comparison suite: 1,187 passed, 19 opt-in tests skipped, zero failures.
- Swift suite: 313 tests, five opt-in checks skipped, zero failures; release build succeeded.
- Rust unit/integration suites: 180 passed, two ignored, zero failures.
- Regressions cover foreground tagging from a paused queue, retaining individually
  paused jobs, a later Pause winning, stale progress state, remount recovery,
  retained pending targets, accepted thumbnail identity and filename refresh.
- All 418 distributable payload entries retain their reviewed classifications;
  read-only DMG inspection matches the exact signed candidate. Third-party bytes,
  public datasets and notices are unchanged from beta 5.

The exact bundled app also passed an isolated end-to-end remount recovery test
using the owner's separately selected local 4B Qwen vision model. A stale identity
paused without consuming the target; checking and explicitly reconfirming the
fixture folder restored access. A new Tag action started from a paused queue,
retained the unrelated held job, committed accepted tags and supplied the same
saved thumbnail to the last-tagged Findrydal event. The original was unchanged and
all owned processes drained cleanly. This was one pinned sample photo, not a
large-library or general model-quality benchmark.

## Signed update and saved data

A real Sparkle trial installed the exact build-86 candidate over an isolated
beta-5 build-85 copy. All 21 fixture-library files stayed unchanged during
installation. Reopening the new worker preserved all 17 authoritative source
tables, including accepted details, People edits and review history. Fixture
originals were unchanged. Customer data was not a trial target.

The trial used loopback-only HTTP with real Ed25519-signed feed and archive and
an external updater harness. The old app used a private updater preferences domain
and was re-signed; the installed new app exactly matched the release candidate.
Production UI save/drain gates have separate tests. Public HTTPS artifact
verification is performed during publication. Prior negative update/rollback
checks were not all repeated. No automatic downgrade is supported.

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
