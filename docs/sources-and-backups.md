# Files, offline previews and backups

People is supported on Mac and Linux. Whole-library backup and restore work on Mac, Linux and Windows from 0.3.0-beta.2. Source archives remain a Mac feature. Keep independent backups of your library and originals.

Original photographs belong in the folders and drives you choose. PhotoFindry's library stores saved details, people corrections and prepared image copies separately.

## When a drive is disconnected

Saved details remain available for search. Prepared offline previews can remain viewable while originals are offline. A preview is a reduced-size cached copy, not an original-photo backup. Reconnect the original source to open full-resolution files or do work requiring those pixels.

PhotoFindry checks registered sources in the background while the companion is running. Returning drives and verified moves can reconnect automatically. Files shows when checks are still in progress, an original has changed, or macOS denied access. An unavailable drive is not proof that photographs were deleted. Do not create a new source or purge old records just to resolve a mount or permission problem.

## Library backup

Use **Backups & archives → Export library backup…** to export a portable library backup. New packages use `.photofindrybackup`; legacy `.photorecallbackup` packages remain readable. These packages preserve supported saved library evidence, not original photo files or model weights.

Keep independent backups of originals. Restore is a deliberate operation: review the selected archive and the recovery information shown by the app. Restore preserves the receiving computer's model file choices. It verifies and stages the selected package before replacing the library, keeps a recovery copy, and leaves imported work paused. Select the whole .photofindrybackup folder, not a file inside it.

## Source archives

On Mac, Files offers scoped source snapshots and additive attachment. Archives can carry supported saved details and selected previews. A source archive is a snapshot, not a live synchronized drive: export a new one after later changes. Original photographs and model weights are not included.

## Cleanup and updates

Library cleanup is separate from original-file deletion. Review scope and warnings before removing records, clearing previews or permanently deleting Removed entries. A preview might be the only remaining image copy. Assistants should never perform cleanup, restore or deletion as an automatic repair.

Routine CLI updates replace versioned program files. Your retained folder-access helper and its local folder choices stay separate from those replacements. Updates do not target customer libraries, originals or model paths. [Installation and updates](https://photofindry.com/docs/install.html) explains the signed beta channel.

## Choose originals once, then follow their status

After restoring a library from another computer or operating system, open Files and choose the corresponding originals folder on this computer. Foreign computer paths are not opened. The native chooser records that local folder choice; PhotoFindry checks its originals in the background. A matching source does not require a second routine confirmation. Verified later folder and file moves keep the same saved photographs and details. Changed or ambiguous originals still need review; matching names and sizes alone do not confirm identical pixels.

The internal access helper is retained across ordinary CLI updates. It opens only for a native folder choice or access restoration; there is no permanent Mac application to keep open. macOS can still revoke access or require consent. If Files reports that specific denial, choose **Allow folder access…** for the affected folder. A disconnected drive, a missing original and an unavailable helper have different recovery actions.

Originals become usable as their checks finish, independently of the camera and location scan. Pause or resume that enrichment separately. Checking a source does not deliberately download known cloud placeholders, erase missing originals, or change accepted tags.

## Recover a job waiting for originals

Jobs keep unavailable targets while other usable photographs continue. Once an original returns at a verified location, eligible source-waiting work continues automatically. A queue or job you explicitly paused stays paused, including after a later source check. Older saved jobs may still need **Resume**.

If a moved folder cannot be verified automatically, choose its originals folder in Files. Check the reason shown before acting: an actual identity conflict needs review, a known absent drive needs reconnecting, and a macOS access denial needs a native folder choice. **Continue with available photos** keeps unavailable targets and survives ordinary Pause/Resume and companion restart.

Large source exports preserve complete People edit and undo history. If an earlier beta failed because an archive piece was too large, retry the export in beta 11. The archive format and saved observations are unchanged; exports still omit original photographs and model weights.

## Jobs when originals move or return

New tagging, metadata, preview, face and improvement jobs keep their requested
photos while originals are unavailable. Other usable photos can continue. A
verified returning original continues its existing job automatically; an unchanged
missing file is not repeatedly processed. Verified moves reconnect in the background; use Files when a genuine identity
conflict or macOS access denial needs your action. Opening recovery does not confirm file identity. Paused jobs
and queues stay paused. Existing older saved jobs may still need their explicit
Resume or recovery action.

Moving entries to Removed skips only those pending processing targets and keeps
their saved data available for Undo. Permanent deletion remains a separate explicit
action. PhotoFindry never deletes original photographs as a recovery measure.
