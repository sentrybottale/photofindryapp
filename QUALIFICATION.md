# Beta qualification

**Release:** 0.2.0-beta.11 · **Application:** 0.2.0 · **Build:** 97 · **Architecture:** arm64

Qualification used this Apple-silicon Mac on macOS 27.0.1. The app declares macOS 14 or later; bundled Core ML ranking requires macOS 15. Other hardware and OS combinations are not claimed tested.

## Local checks

Swift built successfully and its 356-test suite had five optional skips and zero failures. The compatibility Node suite passed 1,208 tests with 28 optional skips and zero failures. The native suite passed 1,217 tests with 18 skips; its sole failure was the release-tool wrapper's 30-second timeout during concurrent model qualification. That complete check passed when repeated alone, including all 28 underlying release-tool tests. Early harness runs used missing engine paths or an outdated queue qualification binary; the final runs use the current native executables. No application behavior was changed to make these checks pass.

The exact bundled production worker tagged a pinned public photo sample with the configured local vision model, saved a valid accepted result and provenance, and drained its owned processes. The bundled face provider detected and embedded a real face from a separate pinned public sample. A separate production-worker check covered automatic eligible-photo discovery, archive opt-in, persistent Pause and compatible scan reuse. Originals were unchanged. This qualifies those workflows, not improved recognition accuracy, broad hardware coverage or general model quality. Earlier build-96 Qwen search and job-control checks remain historical qualification of unchanged model/helper components.

## Build and distribution

All 418 payload entries were reviewed against beta 10. No paths were added or removed. Only compiled app/worker, the build number, update schedule and clean build provenance changed. Engine/helpers, bundled model resources, datasets, notices and entitlements are unchanged. Two stale dependency display-version labels were corrected to match unchanged notice paths. First-party source, the private language benchmark, customer photos, traces and credentials are excluded. Final read-only DMG inspection matched the exact sealed candidate, and the signed artifact inventory passed the pinned public-key verifier.

## Signed update and saved data

A real Sparkle trial installed exact build 97 over an isolated beta-10 build-93 copy. All 15 fixture library files stayed unchanged during installation; all 14 source-store checks matched after reopening the worker. Fixture originals and the owner's installed app were unchanged. The fixture included accepted observations, saved model settings and a human-authored person label.

The trial uses loopback-only HTTP, real Ed25519-signed feed/archive bytes and an external updater harness. Its old app copy uses private preferences and is re-signed; the installed new app exactly matched the release candidate. Initial test installer launches from macOS-protected Documents stalled in the loader before main. Moving the isolated harness to a private cache directory and repeating the complete update/data checks passed. This required no application change. Production update preferences, schedule, opt-out, compatible reminders and save/drain gates have separate native tests. Earlier rollback and negative update cases are not all repeated; automatic downgrade is unsupported.

All 12 public release assets were independently downloaded and matched the authenticated inventory. Pinned-key verification passed for the exact live HTTPS feed and downloaded update archive. The required website hook verified all 43 live files and preserved the signed appcast. The current verification record is in update-verification.json.

## Limits

This is an ad-hoc signed community beta, not Apple Developer ID signed or notarized. A fresh quarantined first launch under a new macOS user remains unqualified. Artifact signatures authenticate bytes, not safety. Signing-key recovery is not newly qualified; the restored existing identity matches the pinned public key, and no replacement key was generated.

There is no OS-enforced filesystem/network sandbox or absolute no-internet guarantee. Downloads, automatic or manual update checks, links and voluntary feedback use the network. Automatic checks default on from beta 11 with a persistent Settings opt-out; download and installation require the user's choice. No app advertising analytics or telemetry is added.

These are local checks, not a claimed GitHub Actions run or a newly completed security scan. Application source stays in the private development repository. This public repository and GitHub's automatic source archives contain distribution documentation and verification tooling only.
