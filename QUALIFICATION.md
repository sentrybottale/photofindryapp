# Beta qualification

**Release:** 0.2.0-beta.13 · **Application:** 0.2.0 · **Build:** 108 · **Architecture:** arm64

Qualification used this 8 GB Apple-silicon Mac on macOS 27.0.1. The application declares macOS 14 or later; bundled Core ML ranking requires macOS 15. Other hardware and OS combinations are not newly qualified.

## Local checks

Swift built successfully and executed 436 tests with 4 optional skips and zero failures. The Rust workspace passed 379 tests with 8 intentional ignores; doctests passed using the pinned compiler. Thirteen website tests passed.

The full comparison Node run passed 1257 tests with 24 skips, two failures and one timeout cancellation. Its three affected checks hit existing timing limits while competing for resources: release tooling, the 62,923-photo archive stress check and native search startup. All 25 cases in those files then passed in isolation with the original time limits and assertions. The native Node suite passed 1270 tests with 14 skips and no failures or cancellations, using two concurrent files and requiring current native executables without silent JavaScript fallback. Runtime preparation finished before Node qualification.

## Runtime evidence

The final candidate's compiled UI, worker, engine, model runtimes, models, datasets and resources match the owner-accepted installed app. All 435 canonical payload entries were compared: only version/provenance metadata and distribution permissions on six unchanged upstream licence files differ. Exact signed worker, engine and model-server bytes match, and the running worker/model executable file identities were checked.

A bounded read-only check of current tagging confirmed fresh accepted output with a real model request after the running model started. No photo descriptions, filenames, names, original paths or generated text were retained in the release receipt. Owner tagging continued: no model was paused or stopped and no inference request was added. A fresh isolated vision/Intern run was not repeated for this beta. The final bundled face helper independently detected and embedded a face from a pinned public sample in a disposable fixture. These checks qualify workflows and runtime continuity, not model or recognition accuracy.

## Build and distribution

All 435 payload entries were reviewed against beta 12. No paths were added or removed; only compiled app/worker/engine and version/provenance metadata changed. All inference runtimes, native helpers, face/Core ML resources, datasets and notices remain unchanged. First-party source, JavaScript, Node, user vision weights, customer data, traces and credentials remain excluded. First-party proprietary terms and upstream third-party terms stay separate. The final release audit has zero blockers; read-only DMG inspection matches the exact sealed candidate, and the authenticated artifact inventory passed pinned-key verification.

## Signed update and saved data

A real Sparkle trial installed exact build 108 over an isolated beta-12 build-107 copy. All fifteen fixture-library files stayed unchanged during installation; all fourteen source-store checks matched after worker reopening. Accepted evidence, a human-authored label and selected model settings survived. Fixture originals and the owner's installed app were unchanged.

The external updater harness uses loopback-only HTTP and real signed feed/archive bytes. It does not repeat a fresh-Mac installation, every negative-update case or earlier rollback coverage. Production UI save/drain gates have separate Swift tests. Published-asset, HTTPS-feed and website verification receipts are recorded separately in update-verification.json after publication.

## Limits

This is an ad-hoc signed community beta, not Apple Developer ID signed or notarized. Signatures authenticate bytes; they do not certify safety. There is no OS-enforced filesystem/network sandbox or absolute no-internet guarantee. Explicit downloads, update checks, links and voluntary feedback use the network. Update reminders retain their Settings opt-out; downloads and installation require the user's choice. No app advertising analytics or telemetry is introduced.

The release checks did not write customer source stores, retag customer photographs or perform cleanup. The real-model continuity check read existing accepted evidence while owner-started tagging continued. These are local checks, not a claimed GitHub Actions run or new security scan. The development repository stays private; this public repository contains distribution documentation and standalone verification tooling only.
