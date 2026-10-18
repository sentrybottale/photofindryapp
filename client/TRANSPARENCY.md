# Release transparency

Each candidate binds the exact client source inventory, source archive, application
artifact, version/build, dependency notices and qualification evidence. Client
builds are independently testable; the private engine implementation is not
reproducible from this repository. Retained Svelte outputs must rebuild identically.
Any native reproducibility claim must include an actual second-build comparison.

PhotoFindry adapts OwnTransit's delivery discipline, not its transport protocol:

- Freeze inputs and enumerate every public path; never export private history.
- Review the public client and private engine changes separately. Record scope,
  completed findings/disposition and exact reviewed revisions.
- Pin build dependencies and preserve upstream notices. No release secrets or
  publishing permission is available to PR checks, including fork contributions.
- Authenticate final artifact inventories before extraction or execution. Verify
  independently from the signing step and reject tampering and wrong keys.
- Qualify first install, update, interruption, rollback, uninstall and data
  preservation using isolated fixtures. Record real model and supported hardware
  checks separately. Old test evidence does not qualify changed artifacts.
- Publish explicit PASS, FAIL or NOT-PERFORMED for each gate, with evidence hashes.
  Missing, failed, skipped or stale evidence cannot become PASS.
- Keep production signing and publication as separate owner-controlled steps.
  Disposable rehearsal keys never qualify the production identity.

The existing Sparkle EdDSA update pin and the release-inventory trust mechanism
are not interchangeable with OwnTransit's independent policy and distribution
signers. A rehearsal must name which trust chain it actually exercised.

No public source, release asset, feed or website promotion is performed by the
local export or rehearsal tooling. The final public receipt must include verified
live asset/feed/website bytes after a separately authorized publication.
