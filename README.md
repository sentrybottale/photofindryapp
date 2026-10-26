# PhotoFindry

**Find the photo you remember. Keep your photos on your computer.**

PhotoFindry tags and searches photographs locally on Apple-silicon Mac, Linux x86-64 and Windows x64. It installs a background companion and opens its workspace in your browser. Your originals, selected models and library stay on your computer. Official builds are free, with no subscription or PhotoFindry account.

## Install the current beta

**0.3.0-beta.1 · build 131** — [release notes and downloads](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.3.0-beta.1).

Choose the instructions for the computer where you will run PhotoFindry:

| Your computer | Before installing | Installation guide | Open it afterward |
| --- | --- | --- | --- |
| **Mac with Apple silicon** | macOS 14 or later; Intel Macs are not supported | [Mac: Terminal setup](INSTALLATION.md#macos-apple-silicon) | [Open PhotoFindry](https://app.photofindry.com/) |
| **Windows 10/11, x64** | Microsoft Visual C++ v14 **x64** runtime and a working Vulkan graphics driver | [Windows: PowerShell setup](INSTALLATION.md#windows-x64) | PhotoFindry in the Start menu |
| **Linux, x86-64** | A systemd user session and the listed command-line tools; `zenity` for the desktop folder chooser | [Linux: terminal setup](INSTALLATION.md#linux-x86-64) | Applications menu or `photofindry open` |

1. Open your platform's guide above and check its prerequisites.
2. Copy the **whole verified setup block** into Terminal or PowerShell. Run PhotoFindry setup as your normal user, without `sudo` or **Run as administrator**. The command checks its installer before running it, then installs the publisher-authenticated companion and login startup.
3. When the local browser workspace opens, choose your vision model and matching projector, add a small photo folder, and start tagging. The [first-run checklist](INSTALLATION.md#after-installation) explains what to expect.

You do not need to clone this repository, compile the client, or download the GitHub **Source code** archives to use PhotoFindry. Fresh Mac users install the CLI companion directly; the older Mac application's only installation role is [migrating existing app users](INSTALLATION.md#existing-mac-app-users).

## Downloads, updates and recovery

The setup commands follow the signed feed for your platform and install the latest compatible published build. Their pinned script addresses stay the same across later releases. See [which release files to use](INSTALLATION.md#which-release-files-do-i-need) if you are browsing the release assets.

Use **Settings → Background companion** for update, startup and recovery controls. Updates replace program files while preserving your library, original photos, saved work and selected model paths. If PhotoFindry cannot open, [rerun the verified setup for your platform](INSTALLATION.md#updates-and-recovery); do not delete your library to repair startup.

First-install trust matters: independently trust the setup instructions, immutable script/verifier hashes and existing publisher key. A checksum copied with a compromised download does not authenticate itself. Read [verification](VERIFY.md). Mac packages use ad-hoc code sealing and separate publisher Ed25519 signatures.

## What is included

- **Local tagging and search.** Select compatible vision GGUF weights and their matching multimodal projector separately; model files can stay in any folder. Search models are bundled.
- **People on Mac and Linux.** Windows does not yet include the face provider.
- **Platform-specific features.** Library backup/source transfer and dedicated CLEF/Intern review runtimes remain Mac-only. See [platform differences and tested scope](INSTALLATION.md#platform-differences).
- **Your originals stay in place.** Cached previews are not backups of original photographs. Keep independent backups.

The clean [client snapshot](client/) is MIT, covering the browser UI, native launcher, companion bridge and allowlisted interaction helpers. Rust engine cores and worker implementations remain proprietary. The implementation repository and history remain private. See [licensing](LICENSE), [privacy](PRIVACY.md) and the [client source inventory](client/CLIENT-SOURCE.json).

[Installation and troubleshooting](INSTALLATION.md) · [Getting started](docs/getting-started.md) · [Website](https://photofindry.com) · [Private feedback](https://photofindry.com/bugs) · [Optional support](https://buymeacoffee.com/photorecall)
