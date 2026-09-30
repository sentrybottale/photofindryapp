# Troubleshooting PhotoFindry beta

Begin with the visible status and the smallest reproducible example. Keep your library, original files and model settings intact.

## macOS will not open the app

Confirm the download is from the official GitHub release and verify its signed inventory. This beta is not Apple-notarized. Follow Apple's per-app Open Anyway flow only if you decide to trust it. Do not disable Gatekeeper, run unknown installer commands, or strip quarantine recursively. Read [installation](https://photofindry.com/docs/install.html).

## No update appears

Use PhotoFindry → Check for Updates. Check your internet connection and the version/build in About PhotoFindry. Beta 6 is app version 0.2.0, build 86; the beta label is visible in the update notes. A current build may correctly report no newer update. Do not replace the configured feed or signing key to force an update.

## Model stopped, unavailable or insufficient memory

Stopped is normal when the saved model is not needed; tagging restarts it automatically. If files moved, choose their current locations in Model Setup. Confirm the vision weights and projector are a supported pair. For memory holds, follow the app's measured recovery controls or choose a smaller compatible model. Do not kill unrelated model processes.

## A job is paused or waiting

Open Jobs & Activity and read its specific reason. A reopened queue stays paused until you resume or start work. Drive, permission, memory and disk holds require their own resolution. Reconnect a source before retrying work that needs its original files. Overnight schedules require the app open and Mac awake.

## Search or a preview is missing

Check folder scope and filters first. Filename search can work before tagging; visual descriptions need saved accepted details. Search indexes are derived and can be preparing independently. A disconnected source may have saved facts but no prepared preview. Do not erase the library or retag everything to hide a readiness or availability problem.

## Report a bug privately

Use the [feedback form](https://photofindry.com/bugs). Email is required for follow-up. Include what happened, where, steps to reproduce, expected behavior and optional redacted screenshots. The server returns a reference after saving the report. Reports are private, not public GitHub issues.

Send Feedback submits directly, without an email app. Keep the receipt for follow-up. Failed submissions retain your draft; retrying unchanged content avoids duplicate reports. Earlier beta 2 builds still use email. No response-time guarantee is implied. Never share your full library, model weights, credentials or unreviewed logs.

Read [privacy and distribution](https://photofindry.com/docs/privacy.html) for what is sent.

For beta 6 support, you can choose **Include diagnostic log with this report** in the app. Review the frozen attachment before submitting; uncheck it to remove it. It contains bounded current-session operational metrics, not historical crash logs.

## Legacy saved-photo search error

Beta 6 handles legacy text that previously caused “Invalid saved photograph” during search. Install the update and retry the same query. Do not delete records, reimport the source or retag the library to repair this error. If it persists, submit a small reproducible report with the app build and optional reviewed diagnostic log.

## Tagging pauses after a drive reconnects

Open **Sources**, check the affected source and review its folder match. A cloud drive remount can change file identities even when the photographs are still in the same folder. Confirm only a folder you recognize as the original collection, then resume the affected job in Jobs & Activity. Saved tags and pending targets are retained. Do not delete the library or bypass the confirmation.

Starting **Tag photographs** puts that new request first and releases an ordinary queue pause. Individually paused jobs and source, memory or disk holds still require their own recovery. The tagging strip shows the current hold and offers **Open Sources** when needed.
