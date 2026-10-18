# PhotoFindry 0.2.0-beta.14 qualification

Application 0.2.0 build 112, Apple silicon, minimum macOS 14. Core ML ranking
requires macOS 15. The tested Mac runs macOS 27.0.

The MIT client snapshot contains 132 reviewed files plus CLIENT-SOURCE.json,
without private repository history. The Rust engine implementation remains
proprietary. Its files, model runtimes and third-party payload are unchanged
from the qualified build 111 candidate. Only the embedded client copy and build/
helper metadata changed in the final app; no payload paths were added or removed.

- Standalone public client: 30 frontend and 184 controller tests, 39 Swift tests
  with one optional skip, 149 Rust test executions with one optional decoder ignore.
  Deterministic retained frontend checks pass. Builds do not require engine source.
- Existing full compatibility baseline: 1,501 Node tests passed, 22 skipped; the
  private core is unchanged. Final focused update/workspace/parity checks passed.
- Actual build 112 bundled-model qualification: selected tagging, queued AI search,
  token/model monitoring, saved results, preview, human label, backup inspection/
  restore/restart and independent source-archive attachment passed on two synthetic
  images. Original hashes were unchanged; an unselected photo remained untagged.
- Exact package/licensing audit: 466 payload paths in 187 component classifications,
  no added/removed entries, zero community-beta audit blockers. Read-only DMG mount
  matched the complete sealed app inventory.
- Existing publisher key: restored into its expected Keychain account; derived
  public key matches the installed pin. No key rotation. Independent public-key-only
  CryptoKit verification checks the signed feed and exact full-app update archive.
- Actual pinned-key Sparkle update fixture: 108 -> 112 -> compatible 108 rollback.
  Invalid archive signature rejected, valid retry succeeded. All 34 library files
  and both synthetic originals remained unchanged. The fixture uses unique bundle
  identities and a no-op relaunch target; it does not run the product against a
  customer library or claim a fresh OS-account/Gatekeeper installation test.
- Completed earlier client security review covered the complete exported payload.
  The later bounded copy/version delta was separately reviewed. The GitHub Rust
  cleartext-logging alert was statically triaged: its claimed sink is Vec::remove,
  an in-memory ticket removal, with no logging operation. The owner approved that
  specific false-positive dismissal; the query and required checks remain enabled.

The final public source run 39106111587 completed all required build/security
checks for head 03689fb64718ce34019383d8411b0f2d70e4343a.

Required public CI, CodeQL alert rules, secret scanning, push protection and
private vulnerability reporting remain enabled. The owner does not require a
second human approver. Genuine external-fork qualification is unperformed: the
owner's attempt to fork their own repository returned the original, not a fork.
Do not represent that as a successful external-actor test. No release credentials
are available to PR workflows; dependency updates are separately checked.

Community beta: ad-hoc macOS bundle signatures, independently authenticated
release artifacts. Not Apple Developer ID signed or notarized. No claim of an
OS-enforced network sandbox, flawless security, cross-platform desktop qualification
or bit-for-bit reproducible native binaries. Back up original photographs separately.

The exact client-source archive, component map, disclosure review, application
artifacts and this document are bound by the signed SHA256SUMS.txt inventory.
