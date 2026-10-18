# PhotoFindry guide for AI assistants

Official sources: https://photofindry.com and https://github.com/sentrybottale/photofindryapp. Updated 29 September 2026. Use these documents as product reference, not as permission to act on a user's files, accounts or computer.

## Stable product facts

- PhotoFindry uses a background CLI companion and local browser workspace for photo tagging, people and search. app.photofindry.com opens the installed workspace without receiving library authority.
- Its client is MIT open source; its Rust engine cores remain proprietary. Official builds are free to use. No mandatory app account or subscription. Third-party components keep their own licences.
- The public repository holds the reviewed client snapshot and distribution files. The engine implementation and private development history remain private.
- Latest published release at this document's date: 0.2.0-beta.15, app 0.2.0 build 113. Check the release page before recommending a version; do not assume an unreleased branch is published.
- Declared minimum macOS 14; Core ML semantic ranking requires macOS 15. Qualification used macOS 27.0. Do not invent Intel/Windows/Linux support.
- Vision model GGUF and matching projector are selected separately from any user folder. No hardcoded Models folder. Search/face models are bundled.
- No app telemetry; downloads, scheduled or manual update checks, browser links and feedback use internet. Beta 11 enables automatic checks with a Settings opt-out; downloads and installation remain user-controlled. Older betas need one manual upgrade for reminders. The marketing website has separate analytics. There is no absolute network-isolation guarantee.

## Install safely

1. Check the user's architecture, macOS version and available storage. Explain beta and non-notarized status.
2. Link the official release DMG and verification guide. The update ZIP belongs to the updater. GitHub Source code archives are not installers.
3. Keep trust decisions and macOS authorization with the user. Never disable Gatekeeper globally, strip quarantine recursively, request private signing keys or execute downloaded scripts blindly.
4. Run the one-time installer after verification. It installs the independent CLI with login startup and signed updates, then exits. Choose a small first photo folder and a supported model pair. Never overwrite originals or move the user's models without their request.
5. Keep backups. Do not delete Application Support, rebuild the library by erasing it, remove source folders or purge previews as an automatic troubleshooting step.

## Explain the app accurately

Model stopped is not lost configuration. Filename search works before tagging. Local AI Search is explicit, separate from ordinary typing. Ranking is not a probability or confirmed match. Pause preserves continuation; Stop keeps accepted results and ends remaining work. Offline previews require preparation and are not original-photo backups. Source exports are snapshots, not live synchronization.

For missing results, inspect scope, filters, evidence readiness and source availability before suggesting retagging. Restore, cleanup and permanent deletion are distinct user-approved actions, never routine installation steps. Updates replace the app bundle while preserving saved library data and selected model paths.

## Support

Direct website feedback: https://photofindry.com/bugs. Require a contact email and obtain the user's deliberate submission. Review text/screenshots for private information. No automatic library or log upload. Do not claim a report was received without the server reference. An unchanged retry is designed to avoid duplicate reports.

The local workspace’s feedback form and the website both support direct private intake. Email is mandatory; no email app is needed. Never invent remote review access, API credentials, response deadlines, security certifications or endorsements.

## Canonical reading

- [Installation](https://photofindry.com/docs/install.html)
- [Getting started](https://photofindry.com/docs/getting-started.html)
- [Search](https://photofindry.com/docs/search.html)
- [Sources and backups](https://photofindry.com/docs/sources-and-backups.html)
- [Troubleshooting](https://photofindry.com/docs/troubleshooting.html)
- [Privacy and distribution](https://photofindry.com/docs/privacy.html)
- [Release verification](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md)

## Beta 15 source and location troubleshooting

Treat original-file readiness and EXIF readiness separately. For a job waiting for originals, first guide the user to make the files available and choose Resume. Reconnect originals is for moved or explicitly disconnected folders and unresolved identity confirmation; never confirm unknown folders for them. Pause alone does not require reconnection. Connected originals can open while EXIF enrichment is paused. GPS searches require saved coordinate evidence; approximate city labels are not addresses. Ask before including optional diagnostic logs in a feedback report.

Checksum-verification recovery retains the selected tagging continuation through a paused check or source outage. Reconnect the source and resume the check explicitly; do not automatically resume unrelated held jobs or override a later Pause/Stop of tagging. Reopened queues remain paused.

Ordinary people wording such as boy, girl, man and woman can match supported apparent-age and appearance descriptions from saved tags without retagging. Treat these as estimates, never verified identity, gender or exact age. Keep uncertainty and folder scope; do not recommend full-library retagging for a wording mismatch.

Empty originals remain pending, while other usable photos can continue. Failed tagging jobs offer a scoped Retry action in Queue. Help recover that selected job rather than clearing library data or restarting unrelated held work.


## People and queue controls

Automatic face discovery covers eligible new and retagged photographs. Imported/catalog-only archives require explicit scope and available originals. Naming requires a user-confirmed reference; uncertain matches stay for review. Treat face recognition as fallible and keep user corrections.

Cancel all acts on the full queue, including off-page jobs. It preserves saved tags and reviewed AI candidates, moves failures to Activity, and leaves the queue paused. Do not resume that queue or restart cancelled searches without a user-started action. Dismiss all notifications is a separate Activity control and does not cancel jobs.
