# Install PhotoFindry beta

PhotoFindry is a free local photo tagging and search workspace for Apple-silicon Macs, with a background companion and browser interface. The current public release is **0.2.0-beta.15, build 113**. This guide was updated on 18 October 2026.

## Requirements

- An Apple-silicon Mac (arm64). There is no qualified Intel, Windows or Linux desktop release.
- macOS 14 or later is the declared minimum. The beta was tested on macOS 27.0; not every supported OS/hardware combination has been tested. Core ML semantic ranking requires macOS 15 or later.
- Sufficient free disk space and memory for your chosen vision model, app and previews. Model needs vary; no universal minimum memory or performance claim is made.
- Compatible vision GGUF weights and matching multimodal projector, chosen separately. Keep them in any folder. Search and face models are bundled.

## Official download

Use the [official beta 15 release](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.2.0-beta.15). The installer is [PhotoFindry-0.2.0-beta.15-arm64-community-beta.dmg](https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.15/PhotoFindry-0.2.0-beta.15-arm64-community-beta.dmg).

The update ZIP is for the built-in updater. GitHub’s automatic source archives contain the open client and distribution files, not a complete runnable application or proprietary engine source. Do not install those archives.

## Verify and install

1. Follow the [signed download verification instructions](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md). Checksums alone do not authenticate a publisher.
2. Open the verified DMG. Drag PhotoFindry.app into Applications.
3. Eject the disk image and launch PhotoFindry from Applications.
4. If macOS blocks this app, decide whether to trust this verified download and follow [Apple's per-app Open Anyway instructions](https://support.apple.com/en-us/102445). Keep this decision with the user. Never disable Gatekeeper globally or recursively strip quarantine.
5. Follow Getting Started. Choose the supported vision model and matching projector. Start with a small folder and keep independent backups.

The app is ad-hoc signed, with separate Ed25519 signatures for downloads and updates. It is **not Apple Developer ID signed, Apple-notarized or on the Mac App Store**. Direct distribution is the selected beta route; it is not equivalent to Apple verification or a guarantee of security.

## Update an existing installation

Existing app users receive a one-time migration installer through the retained signed app feed. Use PhotoFindry → Check for Updates in an older app, or install the current verified download. The installer keeps the library in place, waits for the independent companion to become ready, opens the browser and exits.

After migration, use **Settings → Background companion** in the local browser. Login startup and automatic update checks can each be turned off there. Checks run locally about every six hours while the companion runs; you choose **Install signed update** when ready. Updates authenticate the fixed CLI feed and exact package bytes with the retained publisher key.

The companion keeps its previous verified software for recovery. An interrupted update restores software, never an older library backup. The native opening helper offers **Recover and open** if startup fails; **Recover companion** and **Use previous companion version** are also available in browser Settings when applicable. Keep independent backups of original photographs.

Next: [Getting started](https://photofindry.com/docs/getting-started.html) · [Troubleshooting](https://photofindry.com/docs/troubleshooting.html)

## Browser workspace and native-app migration

The installer runs once. Normal use needs only the background CLI companion and browser; the migration application exits and can move to Trash after successful handoff. Open PhotoFindry from app.photofindry.com. The public page requests only the fixed local launch action; it never receives your library session. The address bar changes to the trusted local workspace.

Existing users keep their library, original photos and chosen model paths. Read the migration screen and let it finish preparing its recovery copy. **Use current beta** opens the verified retained native version if you defer or migration fails; it does not restore old library data. Imported photo sources may need their current originals folder confirmed under Files.

To uninstall, use the local uninstall flow in Settings/About and review its native confirmation. It disables login startup and moves the active verified companion package to Trash while retaining libraries, originals, model files, backups and inactive recovery versions.
