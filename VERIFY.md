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

PhotoFindry verifies the signed beta feed and the full update ZIP with the same
pinned Ed25519 key before installing. From beta 11, automatic checks are enabled on launch and about every six hours while the app runs. Settings → App updates has a persistent opt-out. Downloads and installation require your choice. Earlier betas need one manual upgrade using **Check for Updates…** to obtain reminders. The feed is `https://photofindry.com/updates/beta/appcast.xml`.

The app bundle uses ad-hoc macOS signatures. These are distinct from the
maintainer's Ed25519 download/update signatures and do not provide Apple publisher
validation. This beta is not Apple Developer ID signed or notarized. Signature
verification proves possession of the signing key and exact bytes, not absence of
bugs, enforced isolation or a security guarantee.
