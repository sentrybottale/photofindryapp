# Beta qualification

**Release:** 0.2.0-beta.10 · **Application:** 0.2.0 · **Build:** 93 · **Architecture:** arm64

Qualification used this Apple-silicon Mac on macOS 27.0. The app declares macOS 14 or later; bundled Core ML ranking requires macOS 15. Other hardware and OS combinations are not claimed tested.

## Jobs and source recovery

The queue/source changes passed the full compatibility suite (1,200 passed, 29 opt-in skips), followed by four focused checks for the final resource-recovery adjustment. The final full native suite passed 1,211 tests with 19 opt-in skips and zero failures. Rust passed 174 worker tests and 39 engine/contract tests, with two opt-in tests ignored. Swift builds, the full Swift suite and later focused source-handoff tests passed.

The exact signed release bundle tagged two public sample copies with the configured local vision model. The test paused after the first accepted photo, shut down, reopened and resumed the same job. Saved progress, the available-photos choice and one deferred original survived restart; repeated submission reused the paused job. A simulated filesystem-identity change was recovered using a saved checksum, and source availability returned to available. The original lacking sufficient identity evidence stayed pending. No additional source-recovery jobs were created during the scenario, and all owned workers shut down normally. Original sample copies and upstream samples were unchanged.

This tests a simulated source-identity change with real model inference, not a reproduced live pCloud outage or a general model-accuracy benchmark.

## Build and distribution checks

All 418 payload entries were reviewed against beta 9. Only the compiled app/worker, build number and clean build provenance changed. The engine, helpers, third-party notices, bundled model resources and public datasets are unchanged. First-party source, customer photos, traces and credentials are excluded. Read-only DMG inspection matched the exact signed candidate. The signed artifact inventory passed the pinned public-key verifier.

## Signed update and saved data

A real Sparkle trial installed exact build 93 over an isolated beta-9 build-91 copy. All 21 fixture library files stayed unchanged during installation; all 17 authoritative source tables matched after worker reopen. Fixture originals and the owner's installed app were unchanged. A missing dependency in the external test harness was restored before the successful trial; no app changes were needed.

The trial uses loopback-only HTTP, real Ed25519-signed feed/archive bytes and an external updater harness. The old copy uses private updater preferences and is re-signed; the installed new app exactly matched the signed release candidate. Production UI save/drain gates have separate tests. Published-download and live-feed verification are recorded after publication in update-verification.json. Earlier negative update and rollback cases are not all repeated; automatic downgrade is unsupported.

## Limits

This is an ad-hoc signed community beta, not Apple Developer ID signed or notarized. A fresh quarantined first launch under a new macOS user remains unqualified. Artifact signatures authenticate bytes, not safety. Signing-key backup is owner reported; independent recovery is untested.

There is no OS-enforced filesystem/network sandbox or absolute no-internet guarantee. Explicit downloads, updates, links and voluntary feedback use the network. No app advertising analytics or telemetry is added. Location labels are approximate GeoNames matches, not street-address verification.

These are local checks, not a claimed GitHub Actions run or a newly completed Codex Security scan. Application source stays in the private development repository. This public repository and GitHub's automatic source archives contain distribution documentation and verification tooling only.
