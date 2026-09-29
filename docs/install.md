# Install PhotoFindry beta

PhotoFindry is a free native macOS photo tagging and search app for Apple silicon. The current public release is **0.2.0-beta.2, build 75**. This guide was updated on 29 September 2026.

## Requirements

- An Apple-silicon Mac (arm64). There is no qualified Intel, Windows or Linux desktop release.
- macOS 14 or later is the declared minimum. The beta was tested on macOS 27.0; not every supported OS/hardware combination has been tested. Core ML semantic ranking requires macOS 15 or later.
- Sufficient free disk space and memory for your chosen vision model, app and previews. Model needs vary; no universal minimum memory or performance claim is made.
- Compatible vision GGUF weights and matching multimodal projector, chosen separately. Keep them in any folder. Search and face models are bundled.

## Official download

Use the [official beta 2 release](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.2.0-beta.2). The installer is [PhotoFindry-0.2.0-beta.2-arm64-community-beta.dmg](https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.2/PhotoFindry-0.2.0-beta.2-arm64-community-beta.dmg), 394,519,265 bytes (about 395 MB).

The update ZIP is for the built-in updater. GitHub's automatic “Source code” archives contain distribution documentation, not the application. Do not install those archives.

## Verify and install

1. Follow the [signed download verification instructions](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md). Checksums alone do not authenticate a publisher.
2. Open the verified DMG. Drag PhotoFindry.app into Applications.
3. Eject the disk image and launch PhotoFindry from Applications.
4. If macOS blocks this app, decide whether to trust this verified download and follow [Apple's per-app Open Anyway instructions](https://support.apple.com/en-us/102445). Keep this decision with the user. Never disable Gatekeeper globally or recursively strip quarantine.
5. Follow Getting Started. Choose the supported vision model and matching projector. Start with a small folder and keep independent backups.

The app is ad-hoc signed, with separate Ed25519 signatures for downloads and updates. It is **not Apple Developer ID signed, Apple-notarized or on the Mac App Store**. Direct distribution is the selected beta route; it is not equivalent to Apple verification or a guarantee of security.

## Update an existing installation

Choose **PhotoFindry → Check for Updates…**. Checks are manual. The official feed is [photofindry.com/updates/beta/appcast.xml](https://photofindry.com/updates/beta/appcast.xml). The app authenticates the feed and update ZIP before installation and waits for work and saves to drain.

An update replaces the application bundle, not your photo folders, library databases or selected model paths. Keep a library backup before testing a beta update. Use only a rollback version with documented data compatibility; never delete the library to fix an update problem.

Next: [Getting started](https://photofindry.com/docs/getting-started.html) · [Troubleshooting](https://photofindry.com/docs/troubleshooting.html)
