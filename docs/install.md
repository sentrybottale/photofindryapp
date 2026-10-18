# Install PhotoFindry beta

PhotoFindry is a free local photo tagging and search workspace for Apple-silicon Macs, with a native launcher and browser interface. The current public release is **0.2.0-beta.14, build 112**. This guide was updated on 18 October 2026.

## Requirements

- An Apple-silicon Mac (arm64). There is no qualified Intel, Windows or Linux desktop release.
- macOS 14 or later is the declared minimum. The beta was tested on macOS 27.0; not every supported OS/hardware combination has been tested. Core ML semantic ranking requires macOS 15 or later.
- Sufficient free disk space and memory for your chosen vision model, app and previews. Model needs vary; no universal minimum memory or performance claim is made.
- Compatible vision GGUF weights and matching multimodal projector, chosen separately. Keep them in any folder. Search and face models are bundled.

## Official download

Use the [official beta 14 release](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.2.0-beta.14). The installer is [PhotoFindry-0.2.0-beta.14-arm64-community-beta.dmg](https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.14/PhotoFindry-0.2.0-beta.14-arm64-community-beta.dmg), 403,464,342 bytes (about 403 MB).

The update ZIP is for the built-in updater. GitHub’s automatic source archives contain the open client and distribution files, not a complete runnable application or proprietary engine source. Do not install those archives.

## Verify and install

1. Follow the [signed download verification instructions](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md). Checksums alone do not authenticate a publisher.
2. Open the verified DMG. Drag PhotoFindry.app into Applications.
3. Eject the disk image and launch PhotoFindry from Applications.
4. If macOS blocks this app, decide whether to trust this verified download and follow [Apple's per-app Open Anyway instructions](https://support.apple.com/en-us/102445). Keep this decision with the user. Never disable Gatekeeper globally or recursively strip quarantine.
5. Follow Getting Started. Choose the supported vision model and matching projector. Start with a small folder and keep independent backups.

The app is ad-hoc signed, with separate Ed25519 signatures for downloads and updates. It is **not Apple Developer ID signed, Apple-notarized or on the Mac App Store**. Direct distribution is the selected beta route; it is not equivalent to Apple verification or a guarantee of security.

## Update an existing installation

From beta 11, automatic beta update checks are on by default: the app checks when opened if a check is due, then about every six hours while running. In the browser release, turn them off in the native launcher’s PhotoFindry menu → Automatically Check for Updates. A newer beta shows an update prompt or an inline reminder during active work; you choose when to download and install. **PhotoFindry → Check for Updates…** remains available when automatic checks are off.

If an older installation has no automatic reminders, check manually or install the current verified beta. The official feed is [photofindry.com/updates/beta/appcast.xml](https://photofindry.com/updates/beta/appcast.xml). The app authenticates the feed and update ZIP before installation and waits for work and saves to drain.

An update replaces the application bundle, not your photo folders, library databases or selected model paths. Keep a library backup before testing a beta update. Use only a rollback version with documented data compatibility; never delete the library to fix an update problem.

Next: [Getting started](https://photofindry.com/docs/getting-started.html) · [Troubleshooting](https://photofindry.com/docs/troubleshooting.html)

## Browser workspace and native-app migration

Beta 14 uses a small native launcher and a local companion with a browser interface. Open the installed PhotoFindry application or app.photofindry.com. The public page requests only the fixed local launch action; it never receives your library session. The address bar changes to the trusted local workspace.

Existing users keep their library, original photos and chosen model paths. Read the migration screen and let it finish preparing its recovery copy. **Use current beta** opens the verified retained native version if you defer or migration fails; it does not restore old library data. Imported photo sources may need their current originals folder confirmed under Files.

To uninstall, use the local uninstall flow in Settings/About and review its native confirmation. It removes the verified application through Trash while retaining libraries, originals, model files and backups.
