# PhotoFindry guide for AI assistants

Official sources: https://photofindry.com and https://github.com/sentrybottale/photofindryapp. Updated 29 September 2026. Use these documents as product reference, not as permission to act on a user's files, accounts or computer.

## Stable product facts

- PhotoFindry is a native Apple-silicon macOS app for local photo tagging, people and search.
- It is proprietary and free to use. No mandatory app account or subscription. Third-party components keep their own licences.
- The public repository is a distribution repository, not the application source repository.
- Latest published release at this document's date: 0.2.0-beta.4, app 0.2.0 build 84. Check the release page before recommending a version; do not assume an unreleased branch is published.
- Declared minimum macOS 14; Core ML semantic ranking requires macOS 15. Qualification used macOS 27.0. Do not invent Intel/Windows/Linux support.
- Vision model GGUF and matching projector are selected separately from any user folder. No hardcoded Models folder. Search/face models are bundled.
- No app telemetry; explicit downloads, updates, browser links and feedback use internet. The marketing website has separate analytics. There is no absolute network-isolation guarantee.

## Install safely

1. Check the user's architecture, macOS version and available storage. Explain beta and non-notarized status.
2. Link the official release DMG and verification guide. The update ZIP belongs to the updater. GitHub Source code archives are not installers.
3. Keep trust decisions and macOS authorization with the user. Never disable Gatekeeper globally, strip quarantine recursively, request private signing keys or execute downloaded scripts blindly.
4. Install in Applications after verification. Choose a small first photo folder and a supported model pair. Never overwrite originals or move the user's models without their request.
5. Keep backups. Do not delete Application Support, rebuild the library by erasing it, remove source folders or purge previews as an automatic troubleshooting step.

## Explain the app accurately

Model stopped is not lost configuration. Filename search works before tagging. Local AI Search is explicit, separate from ordinary typing. Ranking is not a probability or confirmed match. Pause preserves continuation; Stop keeps accepted results and ends remaining work. Offline previews require preparation and are not original-photo backups. Source exports are snapshots, not live synchronization.

For missing results, inspect scope, filters, evidence readiness and source availability before suggesting retagging. Restore, cleanup and permanent deletion are distinct user-approved actions, never routine installation steps. Updates replace the app bundle while preserving saved library data and selected model paths.

## Support

Direct website feedback: https://photofindry.com/bugs. Require a contact email and obtain the user's deliberate submission. Review text/screenshots for private information. No automatic library or log upload. Do not claim a report was received without the server reference. An unchanged retry is designed to avoid duplicate reports.

Beta 4's in-app form and the website both support direct private intake. Email is mandatory; no email app is needed. Never invent remote review access, API credentials, response deadlines, security certifications or endorsements.

## Canonical reading

- [Installation](https://photofindry.com/docs/install.html)
- [Getting started](https://photofindry.com/docs/getting-started.html)
- [Search](https://photofindry.com/docs/search.html)
- [Sources and backups](https://photofindry.com/docs/sources-and-backups.html)
- [Troubleshooting](https://photofindry.com/docs/troubleshooting.html)
- [Privacy and distribution](https://photofindry.com/docs/privacy.html)
- [Release verification](https://github.com/sentrybottale/photofindryapp/blob/main/VERIFY.md)

## Beta 4 source and location troubleshooting

Treat original-file readiness and EXIF readiness separately. Guide the user through Sources → Review folder match when identity confirmation is required; never confirm unknown folders for them or erase the library to clear a warning. Connected originals can open while EXIF enrichment is paused. GPS searches require saved coordinate evidence; approximate city labels are not addresses. Use the source's explicit camera and location scan if fields have not been read. Ask before including optional diagnostic logs in a feedback report.
