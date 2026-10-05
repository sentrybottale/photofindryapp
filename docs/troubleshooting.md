# Troubleshooting PhotoFindry beta

Begin with the visible status and the smallest reproducible example. Keep your library, original files and model settings intact.

## macOS will not open the app

Confirm the download is from the official GitHub release and verify its signed inventory. This beta is not Apple-notarized. Follow Apple's per-app Open Anyway flow only if you decide to trust it. Do not disable Gatekeeper, run unknown installer commands, or strip quarantine recursively. Read [installation](https://photofindry.com/docs/install.html).

## No update appears

Use PhotoFindry → Check for Updates. Check your internet connection and the version/build in About PhotoFindry. Beta 10 is app version 0.2.0, build 93; the beta label is visible in the update notes. A current build may correctly report no newer update. Do not replace the configured feed or signing key to force an update.

## Model stopped, unavailable or insufficient memory

Stopped is normal when the saved model is not needed; tagging restarts it automatically. If files moved, choose their current locations in Model Setup. Confirm the vision weights and projector are a supported pair. For memory holds, follow the app's measured recovery controls or choose a smaller compatible model. Do not kill unrelated model processes.

## A job is paused or waiting

Open Jobs & Activity and read its specific reason. For **Waiting for originals**, make the drive or files available and choose **Resume** on that job. Resume retries the saved locations; pausing alone does not require reconnection. Choose **Reconnect originals…** if the folder moved, was explicitly disconnected in Sources, or needs identity confirmation. Confirm only the same originals collection. A later Pause or Stop prevents automatic continuation.

A reopened queue stays paused until you resume or start work. Permission, memory and disk holds still require their specific resolution. Overnight schedules require the app open and Mac awake. Model **Start** can load your saved model while a job waits for originals, but it cannot resolve an unconfirmed folder.

## Search or a preview is missing

Check folder scope and filters first. Filename search can work before tagging; visual descriptions need saved accepted details. Search indexes are derived and can be preparing independently. A disconnected source may have saved facts but no prepared preview. Do not erase the library or retag everything to hide a readiness or availability problem.

## Report a bug privately

Use the [feedback form](https://photofindry.com/bugs). Email is required for follow-up. Include what happened, where, steps to reproduce, expected behavior and optional redacted screenshots. The server returns a reference after saving the report. Reports are private, not public GitHub issues.

Send Feedback submits directly, without an email app. Keep the receipt for follow-up. Failed submissions retain your draft; retrying unchanged content avoids duplicate reports. Earlier beta 2 builds still use email. No response-time guarantee is implied. Never share your full library, model weights, credentials or unreviewed logs.

Read [privacy and distribution](https://photofindry.com/docs/privacy.html) for what is sent.

For beta 10 support, you can choose **Include diagnostic log with this report** in the app. Review the frozen attachment before submitting; uncheck it to remove it. It contains bounded current-session operational metrics, not historical crash logs.

## Legacy saved-photo search error

Beta 10 handles legacy text that previously caused “Invalid saved photograph” during search. Install the update and retry the same query. Do not delete records, reimport the source or retag the library to repair this error. If it persists, submit a small reproducible report with the app build and optional reviewed diagnostic log.

## Tagging pauses after a drive reconnects

Choose **Resume** on the affected job after the drive returns. If filesystem identifiers changed, PhotoFindry can verify a locally available original against its saved checksum without reconnecting the entire source. Originals without enough saved identity evidence still require **Reconnect originals…** and explicit confirmation. Saved tags and pending targets are retained.

Starting **Tag photographs** puts eligible foreground work first and releases an ordinary queue pause. Repeating the same unfinished request reuses its job; an individually paused job keeps its Resume control. **Continue with available photos** keeps unavailable targets pending. Ordinary Pause/Resume preserves that choice across app restarts. Once available work finishes, Resume from the source hold explicitly retries the remaining originals.

## Original verification stops or finishes without continuing tagging

When reconnecting a blocked job, choose **Verify original files** if the review offers saved checksums. A drive or cloud folder must remain available until verification finishes. If the check pauses, reconnect the source and choose **Resume check**. Pausing the verification check retains the requested continuation; successful verification then resumes the selected tagging job from its saved progress. A later Pause or Stop of tagging, or a pause of the whole queue, prevents automatic continuation.

After reopening the app, jobs stay paused. Use **Resume** to retry a job at its saved original locations. If folder verification is needed, start **Reconnect originals…** from that job so the source check is linked to its continuation. One shared source check can serve multiple explicitly linked jobs; unrelated held jobs stay paused. Changed or cloud-only originals remain unresolved until their specific issue is addressed.

## Empty or corrupt original files

Beta 10 continues past empty and corrupt files to process other usable photographs. Empty originals stay pending; reconnect or make the originals locally available, then explicitly Resume the held job. Corrupt images may need repair or replacement outside PhotoFindry. Saved tags are retained.

Failed tagging jobs appear in Queue with **Retry**. Read the final error before retrying. Retry applies to that selected job; unrelated source holds and schedules stay in place.
