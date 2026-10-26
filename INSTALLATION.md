# Install PhotoFindry

**Current release: 0.3.0-beta.1 · build 131.** Installation guide updated 26 October 2026.

PhotoFindry installs a local background companion and opens a browser workspace. The public website does not receive your photos or library session. You do not need a PhotoFindry account, a subscription, a compiler, or a checkout of this repository.

**Jump to your platform:** [Mac](#macos-apple-silicon) · [Windows](#windows-x64) · [Linux](#linux-x86-64)

[First run](#after-installation) · [Release files](#which-release-files-do-i-need) · [Updates and recovery](#updates-and-recovery) · [Older Mac apps](#existing-mac-app-users) · [Troubleshooting](#installation-troubleshooting)

## Choose the right version

| Platform | Supported architecture | Requirements |
| --- | --- | --- |
| macOS | **Apple silicon / arm64** | macOS 14+; Core ML semantic ranking needs macOS 15+ |
| Windows | **Intel/AMD x64 / x86_64** | Windows 10/11; Visual C++ v14 x64 runtime; working Vulkan graphics driver |
| Linux | **Intel/AMD x86-64 / x86_64** | systemd user session; Bash, curl, tar, sha256sum, base64 and systemctl; zenity for the desktop chooser |

There is no Intel Mac or ARM Linux/Windows package. On Mac, **Apple menu → About This Mac** should show an Apple chip. On Windows, check **Settings → System → About → System type** for an x64-based processor. On Linux, `uname -m` should report `x86_64`.

Allow disk space for the program, your chosen model and projector, and library previews. Model memory and performance needs vary; the package download sizes below do not include your chosen vision model. Start with a small folder and keep independent backups of originals.

The commands below use immutable, checksum-pinned setup scripts. Each script then checks its pinned verifier and the platform's publisher-signed release feed before installing the package. **Copy the entire block, including the checksum check.** For first installation, independently trust these instructions and the existing publisher pin in the [verification guide](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md). Do not bypass a failed checksum or accept an unexplained key change.

## macOS (Apple silicon)

### Before you start

Use an Apple-silicon Mac running macOS 14 or later. Normal setup uses macOS's included tools; it does not require Xcode or the Swift compiler. You do not drag an application into Applications for a fresh install.

### Install

1. Open **Terminal** from Applications → Utilities, or search for Terminal with Spotlight.
2. Copy the entire block below, paste it into Terminal and press Return. Run it as your normal account, without `sudo`.
3. Wait for verification, download and installation to finish. The local workspace opens in your default browser. You can close Terminal after setup finishes; the companion continues running.

```sh
( pf_setup=$(/usr/bin/mktemp) || exit
trap '/bin/rm -f "$pf_setup"' EXIT
/usr/bin/curl --disable -fL --proto '=https' --proto-redir '=https' --max-time 60 --max-filesize 65536 https://photofindry.com/installers/sha256/588a19f4d9be78372921c2c2e2ee43f4df04bb127388c5849f980eef7f49c522/install-cli.sh -o "$pf_setup" || { /bin/echo 'PhotoFindry setup could not download. Check your connection, then retry. Your library was not changed.' >&2; exit 1; }
printf '588a19f4d9be78372921c2c2e2ee43f4df04bb127388c5849f980eef7f49c522  %s\n' "$pf_setup" | /usr/bin/shasum -a 256 -c - || { /bin/echo 'PhotoFindry setup did not match its trusted checksum. Nothing was run. Get the complete setup command from https://photofindry.com/docs/install.html and retry.' >&2; exit 1; }
/usr/bin/env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin /bin/bash "$pf_setup" )
```

### Open it again

Visit [app.photofindry.com](https://app.photofindry.com/) and choose **Open PhotoFindry**. Allow your browser to open the local PhotoFindry link if asked. The address changes to the local workspace. Login startup is installed automatically and can be changed in **Settings → Background companion**.

When **Files** asks for a source folder, select the existing originals folder in the native chooser. The retained local access helper keeps that choice across routine updates. If macOS access is revoked, use the specific folder-access action shown in Files; do not delete permission files or originals.

Already using an older native Mac app? See [Existing Mac app users](#existing-mac-app-users).

## Windows (x64)

### Before you start

1. Install Microsoft's **Visual C++ v14 Redistributable, x64**: [official Microsoft downloads and requirements](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist) · [x64 installer](https://aka.ms/vc14/vc_redist.x64.exe). You need the runtime, not the Visual Studio development environment. Microsoft's installer may request administrator approval.
2. Install or update the graphics driver supplied by your GPU manufacturer so Vulkan is available.
3. Use a normal **64-bit Windows PowerShell** window. PhotoFindry setup itself must not run as administrator. The setup uses Windows's `curl.exe` and `tar.exe`.

### Install

1. Open Start, type **Windows PowerShell**, and open it normally. Do not choose **Run as administrator**. If using Windows Terminal, select its Windows PowerShell tab rather than Command Prompt.
2. Copy the whole block below, paste it into PowerShell and press Enter. If the terminal asks about pasting multiple lines, review the block before allowing the paste.
3. Wait until setup reports **Ready**. It installs startup for your account and opens the local browser workspace. You can close PowerShell after setup finishes.

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

The command's execution-policy option applies to the setup child process; it does not change your machine's saved execution-policy setting. On a managed PC, follow your organization's policy if script execution is blocked.

### Open it again

Open **PhotoFindry** from the **Start menu**. The installed companion runs locally and opens the browser interface. It starts when you sign in unless you disable startup in the companion settings.

Windows currently has no People/face provider, portable library backup/source transfer, or dedicated CLEF/Intern review runtime. The configured vision model is selected separately during first run.

## Linux (x86-64)

### Before you start

Use your normal account in a session with a working systemd user manager. The installer checks for `curl`, `tar`, `sha256sum`, `base64` and `systemctl`; Bash is required to run it. Install missing tools using your distribution's package manager before running setup. Install `zenity` for the native desktop folder chooser and use a desktop browser for the local workspace. Optional HEIF/AVIF decoding needs system libheif codecs.

You can check the user manager without changing it:

```sh
systemctl --user show-environment
```

If that command fails, log in to a session with a systemd user manager before continuing. Do not run PhotoFindry setup through `sudo` or from a root shell.

### Install

1. Open your desktop terminal.
2. Copy the complete block below, paste it into the terminal and run it as your normal user.
3. Wait for verification, download and the **Ready** message. Setup installs the user service and opens the browser when desktop opening is available.

```bash
( pf_setup=$(mktemp) || exit
trap 'rm -f "$pf_setup"' EXIT
curl --disable -fL --proto '=https' --proto-redir '=https' --max-time 60 --max-filesize 65536 'https://photofindry.com/installers/sha256/9db6253cb7d39340d98b65df562516997bb82081fd8cea4b2be32005c91ef857/install-linux.sh' -o "$pf_setup" || exit 1
printf '9db6253cb7d39340d98b65df562516997bb82081fd8cea4b2be32005c91ef857  %s\n' "$pf_setup" | sha256sum -c - || exit 1
bash "$pf_setup" )
```

### Open it again

Choose **PhotoFindry** from your applications menu, or run:

```sh
photofindry open
```

If the browser did not open automatically, use that installed entry again. If your shell cannot find `photofindry`, use the applications menu and check your user PATH configuration. The installer also prints the full installed executable path for status checks.

Linux core installation and real-model checks were performed on an x86-64 Ubuntu system. Those headless tests do not establish a fully qualified Linux desktop GUI or performance on every GPU and distribution.

## After installation

1. **Open the local workspace.** Use your platform's installed entry. Its address is local, typically `http://127.0.0.1:…`; the port can change, so use the Open entry instead of depending on a bookmark to an old port.
2. **Choose your model files.** In model setup or **Settings → Local models**, select a compatible vision **GGUF** and its matching **multimodal projector**. The projector must match the model. Keep both files in any folder you choose; you do not have to move them into the program directory. Search models are bundled, while an optional search reranker is selected or explicitly downloaded separately.
3. **Add a small source folder in Files.** Choose the existing originals folder with the platform picker. Original photographs stay in place. Cloud-backed originals need to be available locally when processing reads them.
4. **Start tagging.** Adding a folder alone does not establish that it has accepted tags. Follow progress in Jobs. Saved jobs may reopen paused; choose Resume when you want them to continue.
5. **Try ordinary search, then Local AI Search.** Ordinary search uses saved information. Local AI Search is an explicit model-assisted action.
6. **Check Settings/About** for the installed version and background companion status. For the current release, expect **0.3.0-beta.1 / build 131**; a later signed release may be installed by the same setup command.

See [Getting started](https://github.com/sentrybottale/photofindryapp/blob/main/docs/getting-started.md) and [Search](https://github.com/sentrybottale/photofindryapp/blob/main/docs/search.md) for the full workflow.

## Which release files do I need?

**For normal installation, use the verified setup block above.** It selects and verifies the correct package, registers the companion and installs startup. Downloading a ZIP alone does not perform those steps.

The [0.3.0-beta.1 release](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.3.0-beta.1) contains these program archives:

| Platform | Program archive | Download size, excluding your vision model |
| --- | --- | --- |
| Apple-silicon Mac | `PhotoFindry-0.3.0-beta.1-arm64-companion.zip` | 398,483,420 bytes (about 398 MB) |
| Linux x86-64 | `PhotoFindry-0.3.0-beta.1-linux-x86_64-companion.tar.gz` | 364,357,379 bytes (about 364 MB) |
| Windows x64 | `PhotoFindry-0.3.0-beta.1-windows-x86_64-companion.zip` | 87,194,889 bytes (about 87 MB) |

Other assets have different purposes:

| Files | Purpose |
| --- | --- |
| `install-cli.sh`, `install-linux.sh`, `install-windows.ps1` | Reviewable setup scripts. Use the pinned commands above to verify them before execution. |
| `cli-release.json`, `cli-linux-x86_64-release.json`, `cli-windows-x86_64-release.json` | Signed release metadata used by the installer/updater; not applications to open. |
| `SHA256SUMS.txt`, `SHA256SUMS.ed25519`, verifier binaries and verification guides | Download authenticity/integrity checks. See the verification guide before manual installation. |
| `photofindry-client-source.tar.gz`, `CLIENT-SOURCE.json`, GitHub **Source code (zip/tar.gz)** | Source and inventory for developers; not ready-to-run installers and not the proprietary engine source. |
| `QUALIFICATION.md`, notices and component maps | Tested scope, limitations and licensing information. |
| `qualification-prior-…-release.json` | Test-baseline metadata, not a user installation or downgrade channel. |
| Older Mac DMG/update ZIP assets | The retained native-app migration channel; not fresh CLI installation. |

**Choosing an older release:** the setup blocks follow the current signed feed; they do not pin installation to the version of this guide or an old release page. Do not edit a version number or remove signature checks to force a downgrade. For recovery, use the retained previous verified companion when offered in Settings. A manual historical installation needs that release's instructions and confirmed saved-data compatibility.

## Updates and recovery

In the local workspace, open **Settings → Background companion** to manage startup, update checks and recovery. Background checks do not install by themselves. Explicit **Open PhotoFindry** checks the authenticated feed and can install a newer signed build when owned work can safely drain; offline or busy conditions keep the verified current build usable with a visible reason. You can also choose **Install signed update**.

Updates replace the program, preserving libraries, original photographs, saved work and selected model paths. Accepted maintenance runs independently of the requesting browser or terminal. Closing that requester does not cancel the update.

If PhotoFindry cannot start, **rerun the complete verified setup block for your platform**. It follows the registered library and repairs the program without needing the broken companion to open. Keep the library in place, including a custom data location. If another app still owns it, finish or pause work and quit that app before retrying.

Use **Recover companion** or **Use previous companion version** in Settings when offered. On Mac the native opening helper can offer **Recover and open**. Software recovery never means restoring an older library backup automatically. Do not delete databases, originals or models to repair startup.

## Existing Mac app users

This section is only for users of the older native Mac application. Fresh Mac users should use the CLI setup above.

1. In the older app, choose **PhotoFindry → Check for Updates** to receive the retained one-time migration bridge.
2. If its updater is unavailable, use the [beta 16 migration bridge](https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.16/PhotoFindry-0.2.0-beta.16-arm64-community-beta.dmg), following the [verification guide](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md). This is a separate migration channel, not the current fresh-install package.
3. Let the bridge prepare its verified recovery copy and hand the registered library to the independent companion. It keeps existing data and model paths, opens the browser when ready, and exits.
4. After a successful handoff, use the companion's Open entry. The migration app is no longer the permanent host and can be moved to Trash.

**Use current beta** remains the alternative before migration and after a failure. It opens the retained verified native version without restoring old library data. Read the migration screen before continuing and keep the recovery copy.

## Installation troubleshooting

| What you see | What to do |
| --- | --- |
| Checksum mismatch or **FAILED** | Stop. Nothing should be run from that download. Copy the full current block again; never remove its checksum check. |
| Publisher/signature verification failed | Keep the installed library and software. Check your connection and the official instructions; do not accept a different key to get past the error. |
| Windows reports a missing Microsoft runtime DLL | Install or repair Microsoft's Visual C++ v14 **x64** runtime, then rerun the complete setup block. Do not download individual DLLs from third-party sites. |
| Windows setup says to use a normal user | Close the elevated PowerShell window and reopen Windows PowerShell normally. |
| Linux reports a missing tool or user manager | Install the named dependency, or log in to a session where `systemctl --user show-environment` works. Rerun as your normal user. |
| Linux has no folder chooser | Install `zenity` and use a desktop session. |
| Browser opens an old address or fails to connect | Use the Mac Open link, Windows Start menu, or Linux installed entry again instead of a saved localhost port. If necessary, rerun the verified setup. |
| Another app owns the library | Finish or pause its work, quit that app and retry. Keep its library and lock files. |
| Model or original file unavailable | Select its existing location in Settings or Files. An offline external drive is not a reason to delete saved data. |
| A job is paused | Review its reason and choose Resume when appropriate. Reinstalling should not be used to erase or restart saved work. |

Further help: [Troubleshooting guide](https://github.com/sentrybottale/photofindryapp/blob/main/docs/troubleshooting.md) · [Private feedback](https://photofindry.com/bugs). Share the error and platform; do not post private photos, library databases or model traces in public issues.

## Platform differences

People is included on Mac and Linux; Windows has no face provider. Library backup/source transfer and dedicated CLEF/Intern review runtimes remain Mac-only. Cross-platform customer-library migration is not offered by this release. Keep independent backups of originals on every platform; previews are reduced cached copies, not original-photo backups.

macOS 14 is the declared minimum, with macOS 27.0 used for the published qualification. Linux/Windows core trials used small real-evidence libraries; they do not establish every desktop, codec, GPU or full-library performance combination. Read the [release's QUALIFICATION.md](https://github.com/sentrybottale/photofindryapp/releases/download/v0.3.0-beta.1/QUALIFICATION.md) for the actual tested scope.
