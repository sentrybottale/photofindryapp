# Install PhotoFindry beta

Use the **DMG** on the [official release page](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.2.0-beta.3).
The update ZIP is for PhotoFindry's updater. Verify the download using [the release verification instructions](VERIFY.md).

1. Open the DMG and drag **PhotoFindry.app** to **Applications**.
2. Eject the disk image and open PhotoFindry.
3. If macOS blocks this specific app, use Apple's per-app [Open Anyway instructions](https://support.apple.com/en-us/102445)
   after verifying the release and deciding to trust it. Do not disable Gatekeeper
   globally or recursively remove quarantine.
4. Follow Getting Started, select your supported vision **GGUF** and matching
   **multimodal projector**, and begin with a small photo folder.

Models can be stored in any folder. There is no required `~/Models` location.
The app bundles its search and face models; the main vision model and matching
projector are selected separately. An optional search reranker is selected or
explicitly downloaded separately.

## Requirements and signing

Apple silicon (arm64) is required. The binary minimum is macOS 14.0; the first
beta's actual test platform and limits are recorded in [QUALIFICATION.md](QUALIFICATION.md).
Bundled Core ML semantic ranking requires macOS 15 or later; it is unavailable
on macOS 14.

This GitHub beta is ad-hoc signed and independently signed for download/update
verification. It is not Developer ID signed, Apple notarized or distributed through
the Mac App Store. Ad-hoc signing checks code integrity; the separately pinned
Ed25519 key authenticates the signed release inventory. Neither is a guarantee of security.

## Keep your work safe

Original photographs stay in their source folders. Library previews are reduced
cached copies, not original-photo backups. Keep independent backups of originals.
Use **Library Backup & Storage** for a library backup, or **Sources** for source
archives. An exported source archive is a snapshot; export again after later edits.

**Check for Updates…** is manual. Updates replace the app and bundled runtime;
customer databases, retained previews, originals and selected model paths are not
installer targets. Keep a library backup before trying a beta update or downgrade.
Only use a rollback version whose saved-data compatibility is confirmed in the
release record.
