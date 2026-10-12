# Sources, offline previews and backups

Original photographs belong in the folders and drives you choose. PhotoFindry's library stores saved details, people corrections and prepared image copies separately.

## When a drive is disconnected

Saved details remain available for search. Prepared offline previews can remain viewable while originals are offline. A preview is a reduced-size cached copy, not an original-photo backup. Reconnect the original source to open full-resolution files or do work requiring those pixels.

Use Sources to inspect availability and reconnect a moved folder. An unavailable drive is not proof that photographs were deleted. Do not create a new source or purge old records just to resolve a mount or permission problem.

## Library backup

Use **File → Library Backup & Storage…** to export a portable library backup. New packages use `.photofindrybackup`; legacy `.photorecallbackup` packages remain readable. These packages preserve supported saved library evidence, not original photo files or model weights.

Keep independent backups of originals. Restore is a deliberate operation: review the selected archive and the recovery information shown by the app. Model file choices belong to the host Mac and should not be silently replaced by a restore.

## Source archives

Sources offers scoped source snapshots and additive attachment. Archives can carry supported saved details and selected previews. A source archive is a snapshot, not a live synchronized drive: export a new one after later changes. Original photographs and model weights are not included.

## Cleanup and updates

Library cleanup is separate from original-file deletion. Review scope and warnings before removing records, clearing previews or permanently deleting Removed entries. A preview might be the only remaining image copy. Assistants should never perform cleanup, restore or deletion as an automatic repair.

Routine app updates replace the app bundle. They do not target customer libraries, originals or model paths. [Installation and updates](https://photofindry.com/docs/install.html) explains the signed beta channel.

## Review and confirm original locations

After attaching a source from another Mac or remounting a drive, open Sources and choose the originals folder. **Review originals** checks locations, file sizes, modification dates and availability without reading every original's contents. If matching files need confirmation, review the folder match and confirm only the same originals collection. Names, sizes and dates are human-confirmed attributes, not a new comparison of photo bytes. Historical checksums remain saved; a current known byte mismatch cannot be overridden by folder confirmation.

Full content comparison is a separate explicit action. Quick review and folder confirmation do not silently start a whole-folder camera and location scan. Start or resume **Camera & location** separately in Sources when wanted; tagging also collects metadata from the original it reads. Saved metadata stays usable while originals are offline.

Cloud-only, missing and permission-blocked originals require their respective resolution. Source checks do not deliberately download known cloud placeholders. Previously unconfirmed photos are not silently approved by an app update.

## Recover a job waiting for originals

Make the drive or files available, then choose **Resume** on the affected job. Resume retries saved targets without repeating a whole originals scan. When PhotoFindry asks for identity confirmation, use **Reconnect originals…** from the job, review the matching folder and confirm it. Confirmation continues the linked job while unrelated held jobs remain paused. A later Pause or Stop prevents automatic continuation.

**Continue with available photos** keeps unavailable targets pending across ordinary Pause/Resume and app restart. After usable targets finish, explicit Resume retries the held originals. Saved tags are retained.
