# Beta qualification

**Release:** 0.2.0-beta.3 · **Application:** 0.2.0 · **Build:** 76 · **Architecture:** arm64

The application declares macOS 14.0 or later; qualification used macOS 27.0
(26A428), Apple silicon and 8 GiB memory. Other OS/hardware combinations are not
claimed tested. Bundled Core ML semantic ranking requires macOS 15 or later.

## Feedback and packaging

- Full production bundle build and strict nested signature verification passed.
  All 416 payload entries are classified in 179 components, with no unresolved
  packaging findings. Read-only DMG inspection matched the signed app inventory.
- Focused feedback/updater checks: 16 passed, one opt-in render test skipped in
  the ordinary run. The separate five-state native render check passed.
- Compatibility regression suite: 1,164 passed, 27 opt-in skipped, zero failures.
- Eight service tests passed locally and on the VPS: required email, validation,
  screenshot cleanup, private route isolation, CSRF, escaped text, resource limits
  and concurrent idempotent storage.
- Synthetic website and native reports reached the live HTTPS API. A selected
  screenshot was verified in the SSH-only reviewer. The actual build-76 app
  submitted from an isolated empty library with optional environment disabled;
  that context was absent from the stored report. Test reports were resolved.
- The release's canonical UI executable matches that live-submission-tested build.
  All 20 other native binaries and executable entitlements match beta 2.
- The installer, update ZIP, feed and signed release inventory passed verification
  with the existing pinned public key. Signatures authenticate bytes, not safety.

## Signed update and saved data

A real Sparkle trial installed the exact beta-3 candidate over isolated beta-2
build 75. All 21 fixture-library files remained unchanged during installation.
Reopening the installed worker preserved all 17 authoritative source tables,
accepted details, People edits and review history. Fixture originals and model
paths stayed unchanged. The customer application and library were not targets.

This trial used loopback-only HTTP with real Ed25519-signed feed and archive.
It used an external updater harness; production UI save/drain gates have separate
coverage. Beta 1 previously qualified HTTPS delivery, compatible manual rollback,
insufficient space, bad signatures, corrupt archive, incompatible version/data
contract, unavailable feed and check/download cancellation. Those cases were not
all repeated for beta 3. Authorization denial, wrong bundle identity and
interruption during final replacement remain unqualified. No automatic downgrade
is supported.

## Reused evidence and limits

The engine, models, source schemas, saved-data formats and updater code are
unchanged by beta 3. Prior real-model tagging/refinement and positive face checks
are reused on the identical runtime; they do not establish broad model accuracy.
The rebuilt face-policy manifest updates only its static-archive provenance hash;
the final face helper executable is identical. Build and privacy metadata reflect
the new explicit feedback delivery route.

Bounded prior process observations saw loopback TCP only and can miss brief
connections. There is no OS-enforced network/filesystem sandbox or absolute
no-internet guarantee. Explicit downloads, updates, browser links and voluntary
feedback use the network. No app advertising analytics or telemetry is added.

This beta is ad-hoc signed, not Apple Developer ID signed or notarized. Successful
quarantined first launch under a fresh macOS user remains unqualified. The owner
reports a signing-key backup; independent recovery has not been tested.

The private PR's Codex Security review completed on 29 September 2026 on the
feedback code commit, with no findings reported. The following changes only
clarified documentation and merged the same code. This is not a security
certification. GitHub Actions could not start because of account billing/spending
limits; the validation listed here ran locally. Application source remains private;
this repository and GitHub's automatic source archives contain distribution files.
