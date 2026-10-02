# Beta qualification

**Release:** 0.2.0-beta.8 · **Application:** 0.2.0 · **Build:** 89 · **Architecture:** arm64

Qualification used this Apple-silicon Mac on macOS 27.0. The app declares macOS
14 or later; bundled Core ML ranking requires macOS 15. Other hardware and OS
combinations are not claimed tested.

## Build and recovery checks

- Compatibility suite: 1,180 passed, 29 opt-in tests skipped, zero failures.
- Native-mode suite using the signed bundled worker: 1,190 passed, 19 opt-in tests skipped, zero failures.
- Swift source-recovery suite: 24 passed, zero failures; Swift builds succeeded.
- Regressions cover retaining the selected tagging continuation through an originals-verification outage or paused check, explicit check Resume, saved counts, unrelated held jobs, duplicate completion, a later Pause of tagging, completion before the RPC reply, replacement-folder verification and closing Sources.
- Reviewed all 418 release-payload entries against beta 7. Compiled app/worker/engine, build metadata and Rust dependency inventory changed; updated SQLite dependency notice paths retain identical licence bytes. First-party source and user data are excluded.
- Read-only DMG inspection matches the exact signed candidate.

The final signed bundled worker passed an isolated recovery check with the actual
selected local Qwen vision model. A deleted original stayed pending while a stale
identity held another target. Exact-byte verification reconnected the matching
original; explicitly resuming the same native tagging job saved one accepted tag
result. The missing target remained pending and original bytes stayed unchanged.
The automatic Swift continuation is separately covered by controlled-worker
tests. This release does not claim a newly completed full GUI/model recovery trial
or a general model-quality benchmark.

## Signed update and saved data

A real Sparkle trial installed the exact build-89 candidate over an isolated
beta-7 build-87 copy. All 21 fixture-library files stayed unchanged during the
installation. Reopening the new worker preserved all 17 authoritative source
tables, including accepted details, People edits and review history. Fixture
originals and the owner's installed app were unchanged by the trial.

The trial used loopback-only HTTP, real Ed25519-signed feed/archive bytes and an
external updater harness. The old app copy used private updater preferences and
was re-signed; the installed new app exactly matched the signed release candidate.
Production UI save/drain gates have separate existing tests. Public download and
signed HTTPS feed verification are performed during publication. Earlier negative
update and rollback cases were not all repeated; automatic downgrade is unsupported.

## Limits

Unavailable drives and cloud folders must be reconnected before verification can
finish. Changed originals require review; cloud-only files are not downloaded by
verification. Reopened queues stay paused until explicitly resumed or new work is
started. Checks on small fixtures do not establish every cloud-provider behavior
or large-library performance.

This is an ad-hoc signed beta, not Apple Developer ID signed or notarized. A fresh
quarantined first launch under a new macOS user remains unqualified. Artifact
signatures authenticate downloaded bytes, not their safety. Signing-key backup is
owner reported; independent recovery is untested.

There is no OS-enforced filesystem/network sandbox or absolute no-internet
guarantee. Explicit downloads, updates, links and voluntary feedback use the
network. No app advertising analytics or telemetry is added. Location labels are
approximate GeoNames matches, not street-address verification.

These are local checks, not a claimed GitHub Actions run or a newly completed
Codex Security scan. Application source stays in the private development
repository. This public repository and GitHub's automatic source archives contain
distribution documentation and verification tooling only.
