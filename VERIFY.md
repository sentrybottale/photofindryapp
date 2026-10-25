# Verify a release

PhotoFindry publishes a **SHA256SUMS.txt** inventory and its detached Ed25519
signature, **SHA256SUMS.ed25519**, with every release. This authenticates the
listed bytes against the PhotoFindry update-signing key. A checksum by itself
only detects differing bytes; it does not establish who published them.

The pinned public key is:

```text
G363x/YCT07qbJyyI5minQjp3v+6SEw9sB7dU3pL6cM=
```

It is also recorded in this repository's `update-verification.json` and the app.
Keep a trusted copy. Downloading a replacement key, verification program and
artifacts from the same compromised source would not establish trust. Do not
accept an unexplained key change.

## Full signature and checksum verification

The small [verify-release.swift](verify-release.swift) utility uses Apple's
CryptoKit, contains only public verification data, and makes no network requests.
It requires Apple's Command Line Tools with Swift; they are not required to run
PhotoFindry itself. You can inspect this standalone utility before running it.

1. Download `SHA256SUMS.txt`, `SHA256SUMS.ed25519`, and **all files named in the
   inventory** from the same official release into a new folder.
2. Save `verify-release.swift` from the release repository in that folder. Confirm
   its embedded public key matches your trusted key above.
3. In Terminal, change to that folder and run:

```sh
swift verify-release.swift .
```

Success reports a verified signature and matching hashes for every listed file.
A failure means the files have not been verified; do not install them. Verification
does not run the application. This utility is separate release tooling, not
application or engine source.

For an additional ordinary byte check, macOS also supports:

```sh
shasum -a 256 -c SHA256SUMS.txt
```

That command alone does **not** check the signature or publisher.

## Application updates

Older app versions authenticate the one-time migration installer using the retained
signed app feed at `https://photofindry.com/updates/beta/appcast.xml`. After handoff,
the independent CLI uses `https://photofindry.com/updates/cli/release.json`.
Its Ed25519 envelope authenticates release/build/platform/data contract, archive
URL/size/hash and the complete package inventory. The installed verifier contains
the same trusted publisher pin; the server cannot supply a replacement key.

Settings → Background companion controls login startup and background checks.
Background checks do not install by themselves. Every explicit Open checks the
fixed authenticated feed and installs a newer signed CLI build when saved work
can safely drain. Offline checks or active work keep the verified current
build open with a visible deferred-update reason. You can also choose Install
signed update. Failed or interrupted switches retain verified prior software
and never automatically restore an older library. The local opening helper
offers Recover and open.

The app bundle uses ad-hoc macOS signatures. These are distinct from the
maintainer's Ed25519 download/update signatures and do not provide Apple publisher
validation. Signature
verification proves possession of the signing key and exact bytes, not absence of
bugs, enforced isolation or a security guarantee.
