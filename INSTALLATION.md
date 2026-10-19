# Install PhotoFindry beta

PhotoFindry is a free local photo tagging and search workspace for Apple-silicon Macs, with a background companion and browser interface. The current public release is **0.2.0-beta.18, build 116**. This guide was updated on 19 October 2026.

## Requirements

- An Apple-silicon Mac (arm64). There is no qualified Intel, Windows or Linux desktop release.
- macOS 14 or later is the declared minimum. The beta was tested on macOS 27.0; not every supported OS/hardware combination has been tested. Core ML semantic ranking requires macOS 15 or later.
- Sufficient free disk space and memory for your chosen vision model, app and previews. Model needs vary; no universal minimum memory or performance claim is made.
- Compatible vision GGUF weights and matching multimodal projector, chosen separately. Keep them in any folder. Search and face models are bundled.

## Install the CLI companion directly

New users install the background CLI and use the browser workspace. No PhotoFindry application goes in Applications. No sudo, Xcode, Swift compiler or cloud account is required.

1. Open Terminal.
2. Paste the complete block below and run it once. It verifies the [reviewable setup script](https://photofindry.com/install-cli.sh) before running it. The script downloads and checks the fixed official CLI package before extraction or execution, then installs login startup and signed updates.
3. Your local library opens in the browser. Close Terminal; the companion keeps running. Next time use [app.photofindry.com](https://app.photofindry.com/).

```sh
( pf_setup=$(/usr/bin/mktemp) || exit
trap '/bin/rm -f "$pf_setup"' EXIT
/usr/bin/curl --disable -fL --proto '=https' --proto-redir '=https' --max-time 60 --max-filesize 65536 https://photofindry.com/install-cli.sh -o "$pf_setup" &&
printf '2db2e520d32685e710cc4b0b0a8953698cfade29542954e5f32d79483638094d  %s\n' "$pf_setup" | /usr/bin/shasum -a 256 -c - &&
/usr/bin/env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin /bin/bash "$pf_setup" )
```

The [official beta 18 release](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.2.0-beta.18) contains the [CLI package ZIP](https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.18/PhotoFindry-0.2.0-beta.18-arm64-companion.zip), 397,731,668 bytes (398 MB). The signed CLI envelope and its package inventory are checked by the pinned native verifier before installation. Startup, update checks and recovery are managed in browser Settings → Background companion.

For first installation, independently trust the instructions and their pinned bytes. Commands and hashes copied together from a compromised website do not authenticate themselves. The [signed-release verification guide](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md) records the existing publisher key. Do not accept an unexplained key change. The optional standalone Swift verifier requires Command Line Tools; normal CLI setup and operation do not.

If setup fails, its message explains the next action. Quit an older application if it still owns the library, then rerun the same command. An existing registered library is retained, including custom paths; an existing library needing handover gets a verified recovery copy before opening. Originals and selected models stay in place. The command also repairs a missing browser connection for an already installed companion.

GitHub source archives are not runnable packages. The update ZIP and DMG are retained for the old app's migration/update channel, not fresh installation. No existing library is deleted as a setup or recovery step. This beta uses ad-hoc macOS code signatures and separate publisher Ed25519 signatures; it is not Apple Developer ID signed or notarized.

## Update an existing installation

Existing app users receive a one-time migration installer through the retained signed app feed. Use PhotoFindry → Check for Updates in an older app, or use the [verified one-time migration bridge](https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.16/PhotoFindry-0.2.0-beta.16-arm64-community-beta.dmg) if its updater is unavailable. That bridge is 403,669,494 bytes (about 404 MB) and is only for existing app users. The installer follows this account’s already registered library, including a library outside the default folder. It keeps the library in place, waits for the independent companion to become ready, opens the browser and exits.

After migration, use **Settings → Background companion** in the local browser. Login startup and automatic update checks can each be turned off there. Checks run locally about every six hours while the companion runs; you choose **Install signed update** when ready. Updates authenticate the fixed CLI feed and exact package bytes with the retained publisher key.

The companion keeps its previous verified software for recovery. An interrupted update restores software, never an older library backup. The native opening helper offers **Recover and open** if startup fails; **Recover companion** and **Use previous companion version** are also available in browser Settings when applicable. Keep independent backups of original photographs.

Next: [Getting started](https://photofindry.com/docs/getting-started.html) · [Troubleshooting](https://photofindry.com/docs/troubleshooting.html)

## Browser workspace and native-app migration

Only existing Mac app users need the one-time application bridge. It runs once. Normal use needs only the background CLI companion and browser; the migration application exits and can move to Trash after successful handoff. Open PhotoFindry from app.photofindry.com. The public page requests only the fixed local launch action; it never receives your library session. The address bar changes to the trusted local workspace.

Existing users keep their library, original photos and chosen model paths. Read the migration screen and let it finish preparing its recovery copy. **Use current beta** opens the verified retained native version if you defer or migration fails; it does not restore old library data. Imported photo sources may need their current originals folder confirmed under Files. If macOS asks for fresh folder access after the CLI handoff, Jobs offers Open Files; choose the existing originals folder, then Retry access.

To uninstall, use the local uninstall flow in Settings/About and review its native confirmation. It disables login startup and moves the active verified companion package to Trash while retaining libraries, originals, model files, backups and inactive recovery versions.
