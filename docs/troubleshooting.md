# Troubleshooting PhotoFindry beta

Begin with the visible status and the smallest reproducible example. Keep your library, original files and model settings intact.

## macOS will not open the app

Confirm the download is from the official GitHub release and verify its signed inventory. This beta is not Apple-notarized. Follow Apple's per-app Open Anyway flow only if you decide to trust it. Do not disable Gatekeeper, run unknown installer commands, or strip quarantine recursively. Read [installation](https://photofindry.com/docs/install.html).

## No update appears

Use PhotoFindry → Check for Updates. Check your internet connection and the version/build in About PhotoFindry. Beta 9 is app version 0.2.0, build 91; the beta label is visible in the update notes. A current build may correctly report no newer update. Do not replace the configured feed or signing key to force an update.

## Model stopped, unavailable or insufficient memory

Stopped is normal when the saved model is not needed; tagging restarts it automatically. If files moved, choose their current locations in Model Setup. Confirm the vision weights and projector are a supported pair. For memory holds, follow the app's measured recovery controls or choose a smaller compatible model. Do not kill unrelated model processes.

## A job is paused or waiting

Open Jobs & Activity and read its specific reason. If a job says **Waiting for originals**, choose **Reconnect originals…** in the tagging strip or Jobs. PhotoFindry checks that job’s folder and resumes when its originals are verified. If confirmation is needed, review the folder and choose **Confirm and resume** only when it is the same originals collection. Cancelling keeps the job paused; you can retry. A later Pause or Stop prevents automatic continuation.

A reopened queue stays paused until you resume or start work. Permission, memory and disk holds still require their specific resolution. Overnight schedules require the app open and Mac awake. Model **Start** can load your saved model while a job waits for originals, but it cannot resolve an unconfirmed folder.

## Search or a preview is missing

Check folder scope and filters first. Filename search can work before tagging; visual descriptions need saved accepted details. Search indexes are derived and can be preparing independently. A disconnected source may have saved facts but no prepared preview. Do not erase the library or retag everything to hide a readiness or availability problem.

## Report a bug privately

Use the [feedback form](https://photofindry.com/bugs). Email is required for follow-up. Include what happened, where, steps to reproduce, expected behavior and optional redacted screenshots. The server returns a reference after saving the report. Reports are private, not public GitHub issues.

Send Feedback submits directly, without an email app. Keep the receipt for follow-up. Failed submissions retain your draft; retrying unchanged content avoids duplicate reports. Earlier beta 2 builds still use email. No response-time guarantee is implied. Never share your full library, model weights, credentials or unreviewed logs.

Read [privacy and distribution](https://photofindry.com/docs/privacy.html) for what is sent.

For beta 9 support, you can choose **Include diagnostic log with this report** in the app. Review the frozen attachment before submitting; uncheck it to remove it. It contains bounded current-session operational metrics, not historical crash logs.

## Legacy saved-photo search error

Beta 9 handles legacy text that previously caused “Invalid saved photograph” during search. Install the update and retry the same query. Do not delete records, reimport the source or retag the library to repair this error. If it persists, submit a small reproducible report with the app build and optional reviewed diagnostic log.

## Tagging pauses after a drive reconnects

Choose **Reconnect originals…** on the affected job. A cloud drive remount can change file identities even when the photographs remain in the same folder. The app checks that folder and offers **Confirm and resume** when confirmation is needed. Confirm only a folder you recognize as the original collection. Saved tags and pending targets are retained. Do not delete the library or bypass the confirmation.

Starting **Tag photographs** puts that new request first and releases an ordinary queue pause. Individually paused jobs and source, memory or disk holds still require their own recovery. Cancelling a recovery leaves work paused and can be retried.

## Original verification stops or finishes without continuing tagging

When reconnecting a blocked job, choose **Verify original files** if the review offers saved checksums. A drive or cloud folder must remain available until verification finishes. If the check pauses, reconnect the source and choose **Resume check**. Pausing the verification check retains the requested continuation; successful verification then resumes the selected tagging job from its saved progress. A later Pause or Stop of tagging, or a pause of the whole queue, prevents automatic continuation.

After reopening the app, jobs stay paused. Start recovery again from **Reconnect originals…** on the blocked job so the check is linked to that job. A standalone source check does not resume every paused job. Changed originals and cloud-only files remain unresolved; PhotoFindry does not silently accept different photographs or download an archive to make verification succeed.
