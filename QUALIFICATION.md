# PhotoFindry 0.2.0-beta.15 hotfix qualification

Application 0.2.0 build 113 is a one-time migration installer for the independent
macOS CLI companion. Apple silicon, minimum macOS 14; Core ML ranking requires
macOS 15. Qualification was performed on macOS 27.0.

This is a focused hotfix, as requested by the owner. The unrelated whole-engine
and whole-release regression suites were not repeated. The private engine,
runtime, models and inference-provider payload entries are identical to beta 14.

- Scoped companion tests: 127 passed; two opt-in cases excluded from the ordinary
  run. The signed-candidate archive/tampering case was run separately and passed.
- Migration host: 38 passed and one existing optional test skipped. Browser
  controls, native parser and packaging checks: 45 passed. Client export boundary:
  seven passed. Retained frontend and parity ownership checks passed.
- Real per-user LaunchAgent: startup from its persisted plist without a resident
  Mac app; authenticated browser controls; rejection of unauthenticated actions
  and caller-supplied update URLs; startup/check preferences.
- Signed release verification: wrong signature, changed payload/archive and
  package tampering rejected. Traversal, symlink and duplicate ZIP entries rejected.
- Update/recovery: the updater was killed after durable version publication.
  Verified prior software recovered without restoring an older library. Signed
  successor installation and explicit software rollback succeeded; the highest
  build watermark prevented older signed updates from bypassing replay protection.
- Path-free recovery rebuilt a deliberately removed test launch registration from
  verified installation state and reopened the service. Recovery serializes with
  other maintenance. Browser update results refresh while the panel is visible.
- A copied synthetic tagged library retained every photo row and selected model
  path through migration, update and rollback; its cold recovery copy verified.
  Confirmed uninstall disabled autostart and moved only the active program to
  Trash while keeping the library and original-file bytes.
- The actual installer layout handed off to an independent CLI, retired its
  inactive disposable app copy to Trash, and left the CLI running with its library.
- Actual model qualification on the development package covered selected tagging/
  improvement, queued AI search, saved search/results, preview and human People
  labels, backup/restore/restart, source archive attachment and unchanged originals.
  Later client changes concern recovery and operation feedback; engine/runtime
  bytes remain unchanged. Safari displayed the local lifecycle controls.

The exact 136-file MIT client snapshot plus CLIENT-SOURCE.json excludes private
engine source and repository history. Two new payload paths are the MIT native
signature verifier and the signed CLI release envelope; all 468 payload paths
have exact component classifications. Publisher signing uses the existing key;
no new trust root is downloaded from the website or update server.

Required public client, distribution-boundary and security checks apply to
[the hotfix PR](https://github.com/sentrybottale/photofindryapp/pull/11). Publication
is permitted only after the release head passes those gates. Secret scanning,
push protection, CodeQL rules and private vulnerability reporting remain enabled.
The owner does not require an independent human approver. This hotfix is not a
new independent security audit or a genuine external-fork qualification.

Community beta: ad-hoc macOS signatures and separately publisher-authenticated
artifacts; not Apple Developer ID signed or notarized. No claim of flawless
security, an OS-enforced sandbox, other-Mac acceptance or qualified Linux/Windows
desktop behavior. Independent photo backups remain important.

The signed release inventory binds this record, the client source, component
review, installer and CLI archive. Both the existing app migration feed and fixed
CLI feed retain the same publisher pin. Live delivery and website verification
are recorded separately after publication.
