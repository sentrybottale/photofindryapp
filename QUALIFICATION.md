# Beta qualification

**Release:** 0.2.0-beta.9 · **Application:** 0.2.0 · **Build:** 91 · **Architecture:** arm64

Qualification used this Apple-silicon Mac on macOS 27.0. The app declares macOS 14 or later; bundled Core ML ranking requires macOS 15. Other hardware and OS combinations are not claimed tested.

## People search and tagging

The complete compatibility suite passed 1,188 tests with 29 opt-in skips and no failures. The final sealed native engine/worker suite passed 1,197 checks with 19 opt-in skips; one release-tooling check hit its 30-second timeout under parallel load and passed on an isolated rerun (1,198 passed in total). Swift builds and 72 selected source, queue, tagging-activity and updater checks passed with no failures.

An isolated test with existing saved tags found the expected photo using ordinary people wording, kept quoted terms literal and rejected an unsupported people description. It also worked with background indexing paused. The live saved accepted record remained unchanged. The final signed bundle passed actual configured local vision-model inference on a copied cached preview, followed by ordinary search. A first model attempt returned a compute error with no accepted output; a fresh owned-model repeat passed. No original photograph was edited or retagged in the owner's library.

These are bounded local checks, not a general model-quality benchmark. Apparent age and people descriptions are estimates, not verified identity, gender or exact age. Supported descriptions remain subject to uncertainty and review.

## Build and distribution checks

All 418 release-payload entries were reviewed against beta 8. Changes are compiled app, worker, engine and photo helper, build metadata and provenance. Third-party notices, bundled model resources and public datasets are unchanged. First-party application source and user data are excluded. Read-only DMG inspection matched the exact signed candidate, and signed update qualification passed.

The previously qualified empty/corrupt-original recovery and scoped Queue Retry fixes are included. Empty originals remain pending while usable photos continue; unavailable sources still need explicit recovery. Reopened queues remain paused.

## Signed update and saved data

A real Sparkle trial installed the exact build-91 candidate over an isolated beta-8 build-89 copy. All 21 library files stayed unchanged during installation; all 17 authoritative source tables matched after worker reopen. Fixture originals and the owner's installed app were unchanged.

The trial uses loopback-only HTTP, real Ed25519-signed feed/archive bytes and an external updater harness. The old app copy uses private updater preferences and is re-signed; the installed new app exactly matched the signed release candidate. Production UI save/drain gates have separate tests. Public download and signed HTTPS feed verification are performed during publication. Earlier negative update and rollback cases are not all repeated; automatic downgrade is unsupported.

## Limits

This is an ad-hoc signed beta, not Apple Developer ID signed or notarized. A fresh quarantined first launch under a new macOS user remains unqualified. Artifact signatures authenticate downloaded bytes, not their safety. Signing-key backup is owner reported; independent recovery is untested.

There is no OS-enforced filesystem/network sandbox or absolute no-internet guarantee. Explicit downloads, updates, links and voluntary feedback use the network. No app advertising analytics or telemetry is added. Location labels are approximate GeoNames matches, not street-address verification.

These are local checks, not a claimed GitHub Actions run or a newly completed Codex Security scan. Application source stays in the private development repository. This public repository and GitHub's automatic source archives contain distribution documentation and verification tooling only.
