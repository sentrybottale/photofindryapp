# Beta qualification

**Release:** 0.2.0-beta.5 · **Application:** 0.2.0 · **Build:** 85 · **Architecture:** arm64

The app declares macOS 14 or later; this qualification used Apple silicon on
macOS 27.0. Bundled Core ML semantic ranking requires macOS 15 or later. Other
OS/hardware combinations are not claimed tested.

## Build and regression evidence

- Compatibility suite: 1,178 passed, 28 opt-in tests skipped, zero failures.
- Native-mode comparison suite: 1,188 passed, 18 opt-in tests skipped, zero failures.
- Swift suite: 310 tests, five opt-in checks skipped, zero failures. This includes
  an actual native text-editor identity/cursor test across compact/pinned layouts.
- Rust unit/integration suite: 175 passed, two ignored, zero failures.
- The saved-text search regression reproduces “Invalid saved photograph” with
  beta 4 and passes with the repaired worker while retaining the exact saved body.
- The exact bundled worker passed two GPS/capture-time search checks with a real
  local Qwen reranker, including queued candidate scoring and saved-result reopen.
- All 418 distributable payload entries retain their reviewed classifications;
  read-only DMG inspection matched the exact signed candidate. Third-party bytes,
  public datasets and notices are unchanged from beta 4.
- A fresh 4B tagging smoke exceeded its four-minute test budget and shut down
  cleanly, preserving its original. It is not counted as a successful tagging run.
  Prior tagging qualification is reused; these repairs do not change tagging.

## Signed update and saved data

A real Sparkle trial installed the exact build-85 candidate over an isolated
beta-4 build-84 copy. All 21 fixture-library files stayed unchanged during
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
