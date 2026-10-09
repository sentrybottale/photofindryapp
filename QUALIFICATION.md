# Beta qualification

**Release:** 0.2.0-beta.12 · **Application:** 0.2.0 · **Build:** 107 · **Architecture:** arm64

Qualification used this Apple-silicon Mac on macOS 27.0.1. The application declares macOS 14 or later; bundled Core ML ranking requires macOS 15. Other hardware and OS combinations are not newly qualified.

## Local checks

Swift built successfully and executed 384 tests with five skips and zero failures. The Rust workspace passed 294 tests with eight intentional ignores. Seven focused packaging tests cover native architecture filtering, the pinned runtime archive, safe links and signing-manifest consistency.

The compatibility Node suite passed 1,223 tests with 29 skips; its only failure was the release-tool wrapper's 30-second timeout during concurrent qualification. The complete wrapper passed in a separate repeat. The final native Node suite passed 1,234 tests with nineteen skips and zero failures. Current native executables were required, with no silent JavaScript fallback.

The exact bundled worker tagged two pinned public sample photographs with a configured local vision model, then completed two independent Intern review jobs with sixteen validated decisions and no failures. One owned reviewer process served the jobs and drained at idle. Accepted runs and original sample bytes were unchanged. The bundled face helper detected and embedded a face from a separate pinned sample. These checks qualify workflows, not recognition accuracy or general model quality. The explicit Intern low-memory profile was tested on this Mac; broad physical 8 GB qualification remains pending. The optional CLEF provider is experimental and currently requires at least 24 GiB; its model workflow is not newly requalified by these release checks.

## Build and distribution

All 435 payload entries were reviewed against beta 11. Seventeen added paths contain upstream Intern notices and the pinned arm64-capable optional CLEF runner with legal notices and its manifest. Unused Intel-only libraries and aliases are excluded. Existing face and Core ML resources remain unchanged. First-party source, JavaScript, Node, user model weights, customer data, traces and credentials are excluded. Proprietary and third-party terms remain separate. The final read-only DMG matched the exact sealed app and its signed artifact inventory passed pinned-key verification.

## Signed update and saved data

A real Sparkle trial installed exact build 107 over an isolated beta-11 build-97 copy. All fifteen library files stayed unchanged during installation; all fourteen source-store checks matched after reopening the worker. Accepted evidence, a human-authored label and selected model settings survived. Fixture originals and the owner's installed application were unchanged.

This is an external updater harness using loopback-only HTTP and real signed feed/archive bytes. It does not repeat a fresh-Mac installation, every negative-update case or earlier rollback coverage. Production UI save/drain gates have separate Swift tests. Live publication verification is recorded separately in update-verification.json.

## Limits

This is an ad-hoc signed community beta, not Apple Developer ID signed or notarized. Signatures authenticate bytes; they do not certify safety. There is no OS-enforced filesystem/network sandbox or absolute no-internet guarantee. Explicit downloads, update checks, links and voluntary feedback use the network. Automatic update reminders retain their Settings opt-out; downloads and installation require the user's choice. No app advertising analytics or telemetry is introduced.

No customer library was opened, migrated, retagged or cleaned. These are local checks, not a claimed GitHub Actions run or new security scan. The application repository remains private; this repository and automatic source archives contain distribution documentation and standalone verification tooling only.
