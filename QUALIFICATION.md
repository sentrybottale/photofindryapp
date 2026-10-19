# Beta 17 / build 115 qualification

Scope: retained-library recovery after deleting an older program, failure rollback
of its inactive registration, direct CLI setup, and shorter verified installation.

- Companion unit checks: 133 passed; two explicit opt-in cases skipped. Separate
  handover checks: 28 passed. Swift compatibility build passed.
- Node regression suite: 1,500 passed, 24 skipped; four native-helper checks first
  selected an old application runtime. All four pass against the qualified current
  runtime. Effective coverage is 1,504 passing checks with the same 24 skips.
- Seven client source/export boundary checks and the mapped parity inventory check
  passed. The source snapshot contains 138 allowlisted files plus its inventory;
  no proprietary engine source, private history, libraries or model selections.
- The exact publisher-signed CLI candidate passed real isolated deleted-program
  setup, blocked-library preservation and retry, launchd/local-browser readiness,
  repeat installation and uninstall while retaining its library and original fixture.
- The private engines and bundled runtime are byte-identical to beta 16. The client
  lifecycle changes do not change inference policy, photo schemas or model weights.
- Website checks cover altered/interrupted script downloads, exact shell rendering,
  stale release rejection, CLI-only feeds and retained native migration links.

The real signed setup test uses Application Support for its isolated program and
library, with a private test launch registration. A preliminary Documents-based
program fixture timed out at launchd readiness; arbitrary program locations are
not qualified. No reboot/new-account/other-Mac acceptance or exhaustive new security
review is claimed. Required public CI/security gates remain enforced; publication
is allowed only after their final-head checks pass. Signed release records provide
final artifact and delivery verification, separately from this source test summary.
