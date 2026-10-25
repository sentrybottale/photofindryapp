# Troubleshooting PhotoFindry beta

Begin with the visible status and the smallest reproducible example. Keep your library, original files and model settings intact.

## macOS will not open the app

Confirm the download is from the official GitHub release and verify its signed inventory. Follow Apple's per-app Open Anyway flow only if you decide to trust it. Do not disable Gatekeeper, run unknown installer commands, or strip quarantine recursively. Read [installation](https://photofindry.com/docs/install.html).

## No update appears

After CLI migration, every explicit **Open PhotoFindry** checks the fixed signed feed and installs a newer build when saved work can safely drain. On Mac, background checks can be switched on or off in Settings → Background companion; they do not install by themselves. On Linux/Windows, use Check for update there or Open from your desktop launcher. If the older companion cannot open, rerun the [verified CLI setup command](https://photofindry.com/docs/install.html#install-the-cli-companion-directly) to repair it without erasing the library. Older Mac app versions use PhotoFindry → Check for Updates for the one-time migration. A current build may correctly report no newer update. Never replace the feed or signing key to force one.

## The workspace cannot open after an update

Use **Open PhotoFindry** once and allow the previous companion to finish shutting down. A large library can take tens of seconds to reopen. If the helper reports a failure, choose **Recover and open**. Beta 1 checks the installed program and tries the retained publisher-verified previous version when the current one cannot open; your current library stays in place. If the error includes **PF-OPEN-300**, note its startup stage for a [private support report](https://photofindry.com/bugs) instead of repeatedly reopening. Keep the library and its recovery copy; do not delete them or restore an old backup as an automatic fix.

If the older companion cannot open the workspace to show Settings, use the [verified direct CLI setup](https://photofindry.com/docs/install.html). Running that complete pinned command updates an existing same-library CLI installation even when its background service is stopped. It preserves the library and selected models; no Mac application or old backup restore is required.

## The browser reports Companion unavailable

The workspace retries the same authenticated connection for a short period while keeping your view and unsaved inputs. Choose **Open PhotoFindry** in its connection panel to start or recover the verified companion; choose **Repair installation** if it cannot open. An accepted update keeps running when the browser or opening helper closes. An action whose response was interrupted is not repeated automatically; check Jobs before starting the same work again.

## macOS needs access to a photo folder

Open Files and read the affected folder's status. For a specific macOS permission denial, choose **Allow folder access…** and select its existing originals folder in the native chooser. The retained internal helper keeps that local choice across ordinary CLI updates. Saved tags and pending work are kept; a second routine folder confirmation is not required. Eligible source-waiting work can continue once checks finish, while jobs or queues you paused remain paused.

A disconnected drive must return before it can be read. A missing or changed original is not automatically a permission problem. If Files says the retained access helper is missing or changed, use the offered companion recovery and keep its source-access state; do not delete or replace permission files manually. macOS may still revoke access or require fresh consent. Do not grant Full Disk Access to a shell or disable privacy protections.

## Model stopped, unavailable or insufficient memory

Stopped is normal when the saved model is not needed; tagging restarts it automatically. If files moved, choose their current locations in Settings → Local models. Confirm the vision weights and projector are a supported pair. For memory holds, follow the app's measured recovery controls or choose a smaller compatible model. Do not kill unrelated model processes.

## A job is paused or waiting

Open Jobs and read its specific reason. For **Waiting for originals**, make the drive or files available. A new job on a running queue continues automatically once the original is verified; use **Resume** for an older saved job or a job you paused. Pausing alone does not require reconnection. Choose **Choose this Mac’s originals folder…** if the folder moved, was explicitly disconnected in Files, or needs identity confirmation. Confirm only the same originals collection. A later Pause or Stop prevents automatic continuation.

A reopened queue stays paused until you resume or start work. Permission, memory and disk holds still require their specific resolution. Overnight schedules require the app open and computer awake. Model **Start** can load your saved model while a job waits for originals, but it cannot resolve an unconfirmed folder.

## Search or a preview is missing

Check folder scope and filters first. Filename search can work before tagging; visual descriptions need saved accepted details. Search indexes are derived and can be preparing independently. A disconnected source may have saved facts but no prepared preview. Do not erase the library or retag everything to hide a readiness or availability problem.

Local AI Search saves retrieved candidates before refinement completes. Use **All candidates** to browse that provisional pool while Best so far is still empty. Ranking can take longer on a large library; a candidate is not a confirmed match. **Stop search** retains reviewed matches and saved candidates. Ordinary Search remains available separately.

## Report a bug privately

Use the [feedback form](https://photofindry.com/bugs). Email is required for follow-up. Include what happened, where, steps to reproduce, expected behavior and optional redacted screenshots. The server returns a reference after saving the report. Reports are private, not public GitHub issues.

Send Feedback submits directly, without an email app. Keep the receipt for follow-up. Failed submissions retain your draft; retrying unchanged content avoids duplicate reports. Earlier beta 2 builds still use email. No response-time guarantee is implied. Never share your full library, model weights, credentials or unreviewed logs.

Read [privacy and distribution](https://photofindry.com/docs/privacy.html) for what is sent.

For beta 1 support, you can choose **Include diagnostic log with this report** in the app. Review the frozen attachment before submitting; uncheck it to remove it. It contains bounded current-session operational metrics, not historical crash logs.

## Legacy saved-photo search error

Beta 1 handles legacy text that previously caused “Invalid saved photograph” during search. Install the update and retry the same query. Do not delete records, reimport the source or retag the library to repair this error. If it persists, submit a small reproducible report with the app build and optional reviewed diagnostic log.

## Tagging waits after a drive reconnects

The companion automatically checks returning originals and verified moves. A job waiting only for an unavailable source can continue from saved progress when its original is verified. If you paused the job or queue, choose **Resume** yourself; later filesystem events cannot release your pause. Read the folder's progress and specific reason in Files before choosing a recovery action.

Starting **Tag photographs** puts eligible foreground work first and releases an ordinary queue pause. Repeating the same unfinished request reuses its job; an individually paused job keeps its Resume control. **Continue with available photos** keeps unavailable targets pending. Older saved jobs may still need an explicit Resume. Saved tags and accepted names are retained throughout source checks.

## Original verification stops or finishes without continuing tagging

When reconnecting a blocked job, choose **Verify original files** if the review offers saved checksums. A drive or cloud folder must remain available until verification finishes. If the check pauses, reconnect the source and choose **Resume check**. Pausing the verification check retains the requested continuation; successful verification then resumes the selected tagging job from its saved progress. A later Pause or Stop of tagging, or a pause of the whole queue, prevents automatic continuation.

After restarting the companion, unfinished jobs stay paused. Closing a browser tab or the setup Terminal does not stop the running companion. Use **Resume** to retry a job at its saved original locations. If folder verification is needed, start **Choose this Mac’s originals folder…** from that job so the source check is linked to its continuation. One shared source check can serve multiple explicitly linked jobs; unrelated held jobs stay paused. Changed or cloud-only originals remain unresolved until their specific issue is addressed.

## Empty or corrupt original files

Beta 1 continues past empty and corrupt files to process other usable photographs. Empty originals stay pending; reconnect or make the originals locally available, then explicitly Resume the held job. Corrupt images may need repair or replacement outside PhotoFindry. Saved tags are retained.

Failed tagging jobs appear in Queue with **Retry**. Read the final error before retrying. Retry applies to that selected job; unrelated source holds and schedules stay in place.


## How do I stop the whole backlog?

Open **Jobs & activity → Queue → Cancel all** and read the confirmation. It stops active work and cancels waiting, paused and scheduled jobs across the whole saved queue, including jobs that are not on the visible page. Accepted tags and reviewed AI matches are kept; failed jobs leave Queue and remain in Activity. The queue stays paused after reopening. Resume queue or start new work when ready. Cancelled AI searches cannot resume their scoring; start a new search instead.

**Dismiss all** in Activity clears notifications only. It does not cancel processing or delete saved results.

## Why are faces still missing or unnamed?

Open **People** and check discovery progress and scope. Resume discovery if it is paused. Make the original folder available or reconnect it in Files when needed. Imported and catalog-only archives are outside the default automatic scope; choose the intended folder or library explicitly. Naming a reference helps match faces that have been found, but cannot identify someone whose face was not detected. Small, obscured or ambiguous faces can remain undetected or need review. This beta repairs automatic discovery and review; its recognition models and matching thresholds are unchanged.

## Open PhotoFindry starts an older app

Rerun the [direct CLI setup](https://photofindry.com/docs/install.html) to repair the companion link, then
return to app.photofindry.com. CLI setup uses the current registered
library and finishes after the background companion is ready. Keep your library and
recovery copy; deleting data or resetting all macOS application associations is
not a recovery step.

## A photo improvement answer was rejected

Update to the current companion and retry the failed job. Invalid model answers
receive one automatic correction using the same saved claims and image scope.
Accepted tags are kept. If the correction also fails, choose another photo review
model in Settings and retry that job. Validation details remain available in Jobs.

## AI search fails while preparing candidates

The current release retains long saved evidence in bounded scoring passages.
Install the update and run the search again; do not retag or erase the library.
All retrieved candidates remain available, and ranking does not confirm a visual
match. Jobs shows the actual failure and retry action if another issue remains.
