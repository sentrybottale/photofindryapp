# Install PhotoFindry beta

PhotoFindry is a free local photo tagging and search workspace for Mac, Linux and Windows, with a background companion and browser interface. The current release is **0.3.0-beta.1, build 131**. This guide was updated on 24 October 2026.

Choose your setup: [Mac](https://photofindry.com/docs/install.html#macos-apple-silicon), [Linux](https://photofindry.com/docs/install.html#linux-x86-64), or [Windows](https://photofindry.com/docs/install.html#windows-x86-64).

## Requirements

- Mac: Apple silicon (arm64), macOS 14+. Linux: x86_64 with a systemd user session. Windows: 10/11 x64, as a normal user. Use the matching platform setup below; Intel Macs and ARM Linux/Windows are not supported.
- macOS 14 or later is the declared minimum. The beta was tested on macOS 27.0; not every supported OS/hardware combination has been tested. Core ML semantic ranking requires macOS 15 or later.
- Sufficient free disk space and memory for your chosen vision model, app and previews. Model needs vary; no universal minimum memory or performance claim is made.
- Compatible vision GGUF weights and matching multimodal projector, chosen separately. Keep them in any folder. Search models are bundled. Face models are included on Mac and Linux.

## Install the CLI companion directly

### macOS · Apple silicon

New users install the background CLI and use the browser workspace. No PhotoFindry application goes in Applications. No sudo, Xcode, Swift compiler or cloud account is required.

1. Open Terminal.
2. Paste the complete block below and run it once. It verifies the [reviewable setup script](https://photofindry.com/installers/sha256/588a19f4d9be78372921c2c2e2ee43f4df04bb127388c5849f980eef7f49c522/install-cli.sh) before running it. This immutable setup entry checks its pinned native publisher verifier, authenticates the latest signed CLI release, and checks the exact package before extraction or execution. Then it installs login startup and signed updates. The same saved command keeps working for later releases.
3. Your local library opens in the browser. Close Terminal; the companion keeps running. On Mac, next time use [app.photofindry.com](https://app.photofindry.com/).

```sh
( pf_setup=$(/usr/bin/mktemp) || exit
trap '/bin/rm -f "$pf_setup"' EXIT
/usr/bin/curl --disable -fL --proto '=https' --proto-redir '=https' --max-time 60 --max-filesize 65536 https://photofindry.com/installers/sha256/588a19f4d9be78372921c2c2e2ee43f4df04bb127388c5849f980eef7f49c522/install-cli.sh -o "$pf_setup" || { /bin/echo 'PhotoFindry setup could not download. Check your connection, then retry. Your library was not changed.' >&2; exit 1; }
printf '588a19f4d9be78372921c2c2e2ee43f4df04bb127388c5849f980eef7f49c522  %s\n' "$pf_setup" | /usr/bin/shasum -a 256 -c - || { /bin/echo 'PhotoFindry setup did not match its trusted checksum. Nothing was run. Get the complete setup command from https://photofindry.com/docs/install.html and retry.' >&2; exit 1; }
/usr/bin/env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin /bin/bash "$pf_setup" )
```

The [official beta 1 release](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.3.0-beta.1) contains the [CLI package ZIP](https://github.com/sentrybottale/photofindryapp/releases/download/v0.3.0-beta.1/PhotoFindry-0.3.0-beta.1-arm64-companion.zip), 398,483,420 bytes (398 MB). The signed CLI envelope and its package inventory are checked by the pinned native verifier before installation. Startup, update checks and recovery are managed in browser Settings → Background companion.

For first installation, independently trust the instructions, immutable setup bytes and native verifier pin. Commands and hashes copied together from a compromised website do not authenticate themselves. The [signed-release verification guide](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md) records the existing publisher key. Do not accept an unexplained key change. The optional standalone Swift verifier requires Command Line Tools; normal CLI setup and operation do not.

If a checksum reports **FAILED**, nothing was run. A command copied from an older page may have used the former changing setup address. Copy the complete current block above once; do not remove or bypass its checksum. New commands use retained immutable addresses, so later releases do not invalidate their trusted bytes.

If an older companion cannot open, rerun the same verified setup command above. It authenticates the latest compatible CLI release, even when the command was saved before that release. It follows the registered library and repairs the program without requiring the broken companion to launch. If setup fails, its message explains the next action. Quit an older application if it still owns the library, then retry. An existing registered library is retained, including custom paths; an existing library needing handover gets a verified recovery copy before opening. Originals and selected models stay in place. The command also repairs a missing browser connection for an already installed companion.

GitHub source archives are not runnable packages. The update ZIP and DMG are retained for the old app's migration/update channel, not fresh installation. No existing library is deleted as a setup or recovery step. This beta uses ad-hoc macOS code signatures and separate publisher Ed25519 signatures; it is not Apple Developer ID signed or notarized.

## Retained folder access

When Files asks for originals, select their existing folder in the native chooser. PhotoFindry keeps that local choice in an internal access helper separate from versioned CLI files, so ordinary updates retain the helper's identity. The helper is short-lived; you do not install a permanent Mac app or need an Apple Developer account.

This does not bypass macOS privacy decisions. If access is revoked or the stored choice is invalid, Files gives the affected folder a specific action. Keep saved libraries and source-access state during recovery. Never delete original photographs or permission files to force a reconnect.

## Update an existing installation

Existing app users receive a one-time migration installer through the retained signed app feed. Use PhotoFindry → Check for Updates in an older app, or use the [verified one-time migration bridge](https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.16/PhotoFindry-0.2.0-beta.16-arm64-community-beta.dmg) if its updater is unavailable. That bridge is 398,483,420 bytes (about 398 MB) and is only for existing app users. The installer follows this account’s already registered library, including a library outside the default folder. It keeps the library in place, waits for the independent companion to become ready, opens the browser and exits.

After migration, use **Settings → Background companion** in the local browser. Login startup and background update checks can each be turned off there. Every explicit **Open PhotoFindry** checks the fixed authenticated CLI feed. A newer signed build installs before opening when owned work can safely drain. Offline checks or active foreground work open the verified current build with a visible deferred-update reason. Background checks run about every six hours and do not install by themselves; you can also choose **Install signed update**. Updates authenticate exact package bytes with the retained publisher key.

Signed update installation runs independently of the browser and opening helper. Closing either does not cancel an accepted update; Open PhotoFindry follows its progress. A failed native Open-link repair keeps a healthy workspace running and shows a repair action in Settings.

The companion keeps its previous verified software for recovery. An interrupted update restores software, never an older library backup. The native opening helper offers **Recover and open** if startup fails; **Recover companion** and **Use previous companion version** are also available in browser Settings when applicable. Keep independent backups of original photographs.

Next: [Getting started](https://photofindry.com/docs/getting-started.html) · [Troubleshooting](https://photofindry.com/docs/troubleshooting.html)

## Browser workspace and native-app migration

Only existing Mac app users need the one-time application bridge. It runs once. Normal use needs only the background CLI companion and browser; the migration application exits and can move to Trash after successful handoff. Open PhotoFindry from app.photofindry.com. The public page requests only the fixed local launch action; it never receives your library session. The address bar changes to the trusted local workspace.

Existing users keep their library, original photos and chosen model paths. Read the migration screen and let it finish preparing its recovery copy. **Use current beta** opens the verified retained native version if you defer or migration fails; it does not restore old library data. Imported photo sources may need their current originals folder confirmed under Files. If macOS asks for fresh folder access after the CLI handoff, Jobs offers Open Files; choose the existing originals folder, then Retry access.

To uninstall, use the local uninstall flow in Settings/About and review its confirmation. It disables login startup and moves the active verified companion package to Trash while retaining libraries, originals, model files, backups and inactive recovery versions.

<!-- windows-x86_64 setup -->
## Windows x86_64

Use normal PowerShell, without administrator elevation. Install Microsoft Visual C++ v14 x64 and a working Vulkan graphics driver first. People is unavailable on Windows. Open PhotoFindry from the Start menu after setup.

```powershell
$pfSetup = Join-Path ([IO.Path]::GetTempPath()) ('photofindry-' + [guid]::NewGuid().ToString('N') + '.ps1')
try {
  curl.exe --disable -fL --proto '=https' --proto-redir '=https' --max-time 60 --max-filesize 65536 'https://photofindry.com/installers/sha256/24203d48b8b86da3c782e8afa94df000dbf2a8a7aadaa49197e2d58e6d17cefa/install-windows.ps1' -o $pfSetup
  if ($LASTEXITCODE -ne 0) { throw 'PhotoFindry setup could not download.' }
  if ((Get-FileHash -LiteralPath $pfSetup -Algorithm SHA256).Hash.ToLowerInvariant() -ne '24203d48b8b86da3c782e8afa94df000dbf2a8a7aadaa49197e2d58e6d17cefa') { throw 'PhotoFindry setup checksum did not match. Nothing was run.' }
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File $pfSetup
  if ($LASTEXITCODE -ne 0) { throw 'PhotoFindry setup did not finish. Your library is kept.' }
} finally { Remove-Item -LiteralPath $pfSetup -Force -ErrorAction SilentlyContinue }
```

<!-- /windows-x86_64 setup -->

<!-- linux-x86_64 setup -->
## Linux x86_64

Use a normal user with a systemd desktop session. People is included; HEIF/AVIF need system libheif codecs. Open PhotoFindry from your applications menu or run photofindry open.

```bash
( pf_setup=$(mktemp) || exit
trap 'rm -f "$pf_setup"' EXIT
curl --disable -fL --proto '=https' --proto-redir '=https' --max-time 60 --max-filesize 65536 'https://photofindry.com/installers/sha256/9db6253cb7d39340d98b65df562516997bb82081fd8cea4b2be32005c91ef857/install-linux.sh' -o "$pf_setup" || exit 1
printf '9db6253cb7d39340d98b65df562516997bb82081fd8cea4b2be32005c91ef857  %s\n' "$pf_setup" | sha256sum -c - || exit 1
bash "$pf_setup" )
```

<!-- /linux-x86_64 setup -->
