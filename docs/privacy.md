# Privacy, licensing and direct beta distribution

PhotoFindry and its local engine are proprietary and free to use for personal and business use. There is no mandatory app account, subscription, advertising analytics or app telemetry. Optional [Buy Me a Coffee support](https://buymeacoffee.com/photorecall) does not unlock features.

## Local photo work

Photo tagging, face processing and search inference run locally on your Mac. Original photographs stay in their folders. Local saved work remains readable and exportable. Backups and source archives exclude original photographs and model weights; keep your own backups of those files.

The current app is not an OS-enforced network sandbox, and local processing is not a guarantee that software has no security defects. Downloads, automatic or manual update checks, browser links and voluntary feedback require network connections. Automatic checks are enabled by default from beta 11 and can be disabled in Settings → App updates. Checks send no library or model data; download and installation remain user choices. Hosts can receive ordinary connection information.

## Voluntary feedback

The form at photofindry.com/bugs sends your required email, typed report and chosen screenshots over HTTPS to the PhotoFindry server for private review and follow-up. It returns a reference only after storage succeeds. Server copies remove original screenshot metadata and filenames; visible image content remains visible. Redact private details before uploading.

No photo library, saved queries or diagnostic logs are collected automatically. Reports remain until the team explicitly removes them. Reports and screenshots are private and accessible only to the PhotoFindry team. Short-lived keyed network identifiers limit abuse. The feedback form has no analytics or advertising trackers.

In beta 11, Send Feedback submits directly to this private service and returns a report reference. Earlier beta 2 builds used an email draft; update to use direct native submission.

## Website and app are distinct

The marketing homepage uses optional PostHog website analytics only after you accept analytics cookies. PostHog uses an EU endpoint and identified-only profiles. Session recording and automatic interaction capture are disabled. That is separate from the app. The documentation pages and feedback form do not load this analytics script. Infrastructure may maintain operational connection logs; “no app tracking” is not a promise that no server can observe a network request.

## Why GitHub rather than Apple's stores and notarization?

The owner chose an OwnTransit-style direct GitHub beta distribution path. Mac App Store qualification, Developer ID signing and Apple notarization are separate tracks and are not part of this release. This is a distribution decision, not a claim that Apple's checks are unnecessary or that proprietary/free software is automatically safe.

The app bundle is ad-hoc signed. Separate pinned Ed25519 signatures authenticate the release inventory, update feed and archive. They establish approved bytes and possession of the signing key, not absence of vulnerabilities. Follow the official verification and per-app installation guidance.

The public [photofindryapp repository](https://github.com/sentrybottale/photofindryapp) contains distribution documentation and official binaries. Application source stays private. Third-party licences and notices remain included. Read the [licence](https://github.com/sentrybottale/photofindryapp/blob/main/LICENSE) and [qualification record](https://github.com/sentrybottale/photofindryapp/blob/main/QUALIFICATION.md).

## Optional diagnostic attachment

Beta 11 adds an unchecked **Include diagnostic log with this report** option in the app feedback form. It includes up to 200 recent operational events from the current session, covering at most 30 minutes. It contains selected job/model states and numeric progress, memory and token counters, excluding raw messages, names, filenames, queries, paths, photo identifiers and image content. It is not a full crash log.

Opting in freezes the attachment for review. Refresh log captures a newer copy; unchecking removes it. Only Submit Feedback sends the reviewed attachment. Retries reuse that copy. Earlier sessions and historical disk logs are not collected. Diagnostic attachments stay with the private report and follow the same retention policy.

## Cookies and storage

The homepage offers **Accept all**, **Reject optional** and **Settings**. Optional analytics stays off until you accept it; rejecting it does not affect downloads or other site features. Change or withdraw your choice using **Cookie settings** in the homepage footer or [open cookie settings](https://photofindry.com/#cookie-settings).

A necessary first-party cookie, `photofindry_consent_v1`, remembers your choice for 180 days on this site only. If you accept analytics, PostHog may store a first-party browser identifier in cookies and local storage to measure visits. Its cookies last up to 180 days; local storage persists until cleared. Rejecting analytics removes this site's PostHog cookies and browser storage and stops further collection; it does not erase information already sent. Do Not Track is respected.

If JavaScript is disabled, optional analytics does not load. Documentation and the feedback form do not load analytics. These website choices do not enable tracking in the Mac app.
