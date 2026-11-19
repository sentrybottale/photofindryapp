# PhotoFindry 0.3.0-beta.9 / build 145 qualification

Mac build 145 was published on 19 November 2026. Linux keeps 0.3.0-beta.8 / build 142 and Windows keeps 0.3.0-beta.2 / build 132; their receipts are in the earlier releases. Build numbers 143 and 144 were used only for unpublished Windows test builds. The existing beta 16 Mac-app migration channel is unchanged.

## Changes

- **Retry Needs attention.** One confirmed action queues every photograph in the current source or folder whose tagging attempt failed, independently of the page and search text. Failed retags keep their tags if the retry fails again; photographs listed only for an improvement issue are not retagged.
- **Quiet automatic review.** A failed automatic improvement batch no longer stays in the Jobs queue or adds a notification; it stays in history with Retry. Failed automatic batches left by earlier builds leave the queue when the library opens. A whole-job automatic failure waits six hours before the next automatic batch. Improvement started by the user reports failures as before.
- **Cloud remounts.** Re-verifying the same original under the same folder grant after a provider remount no longer records a source move. A source with a large location history offers an explicit, stoppable **Compact location history…** for existing repeated entries and **Reclaim … of catalog space…** when at least 64 MB of catalog pages are free.

No library migration, retagging or folder review is required.

## Exact candidate checks

The candidate was built and signed once from source revision 164369f. Every check below ran against those exact signed bytes. The Swift helper programs were compiled by Swift 6.4.2 and differ from build 142's bytes although their sources are unchanged. The setup command's pinned publisher verifier is unchanged since build 142 and authenticates this release.

- **Signed lifecycle:** 17 of 17 checks from the published build 142 to candidate 145: upgrade, rollback, anti-replay, signature rejection, interrupted switch recovery, serving policy and preserved-data uninstall/reinstall. The first run stopped after 6 passes: the update's `--open-after` step reported "The local browser opener did not respond" because the default browser was not running and its cold start exceeded the opener's 5-second wait. The software update itself had completed. A re-run with the browser already running passed 17 of 17. A cold browser start can therefore still produce this message after an update; choosing Open PhotoFindry again opens it.
- **Source matrix (revision 164369f, against the exact package):** 1,622 JavaScript tests with 1,596 passes, zero failures and 26 optional skips; 249 Python tests (two opt-in skips); 933 Rust tests with zero failures (18 ignored); and the Swift build. The final run used a byte-identical copy of the signed package (every file and the signed inventory hash checked). Four earlier attempts each stopped on one timing or shared-resource test and are kept on record: a native folder-access lock test while the real-model core check ran at the same time; the private JavaScript comparison model's write count (10 instead of 9; 20 of 20 passes alone, also seen at build 142); an update-preparation test that correctly refused to take the package's update lease while the user-journey suite's companions were running from the same package directory; and a worker test with a 5-second fairness bound while the user-journey suite was tagging with the real model (5 of 5 passes alone).
- **Real removable-volume churn:** 7 of 7, no skips (eject, flapping, forced eject, bulk deletion, stale mount point, same-named drive).

The installed candidate ran the real-core checks on a 93,807-photo isolated library on an Apple M5 MacBook Pro with 32 GiB RAM and macOS 27.2.1, using Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 6,883 matches; 4.76 s |
| Completed scoped AI | 1 candidate; 24.1 s |
| Real tagging | 1 copied photograph; 32 observations; 26.9 s |
| Whole-pool retrieval to saved scores | 22,192 candidates; 27.1 s |
| Active Stop | 1.14 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

The Karate user-journey suite ran against the exact signed package on separate copies of the same library and passed all 7 scenarios with no skips (3,506 s). Its first run passed 4 scenarios and failed 3: the test harness's own loopback WebDAV mount of the simulated cloud drive stopped answering inside macOS before PhotoFindry had used it, and stayed blocked for about 16 hours; the scenarios queued behind it timed out. The re-run on the same bytes passed every scenario:

| Journey | Result |
| --- | --- |
| Source remount (2 scenarios) | A cloud folder returned in 2.56 s after remount and a USB drive folder in 2.07 s after replug; a replaced original on the drive was flagged while the rest returned |
| Job control | Individual pause, queue pause, Quit and reopen, Resume and Stop of real tagging jobs |
| Install lifecycle | Prior release, signed update to the candidate in 12.9 s, uninstall and reinstall with library, jobs and model settings kept |
| Library from another Mac | 3,000 photos reconnected by relative location in 4.14 s reading 1.4 MB of 224.6 MB; after unplugging and replugging their drive, both folders returned reading 1.35 MB, in 29.3 s (6.2 s in the first run on the same bytes, 4.1 s at build 142; the slower return was not explained) |
| Tagging, search and AI | Real tagging 35.7 s; after a "man in suit" search, semantic coverage stayed at 65,189 photos; scoped AI 1 candidate; whole-library AI kept 22,083 candidates (floor 22,000); Stop 0.97 s |
| Showcase lens | Lens built in 49.0 s and saved; Local AI Search through it kept 30,807 candidates with the lens members first and refined; deleted; uninstall left the library unchanged |

These timings were taken on one Mac shared with the owner's normal PhotoFindry installation and other qualification work; they are not throughput claims.

## Scope of the new features

- **Retry Needs attention** is covered by engine tests (only failed tagging attempts are queued, deduplicated, and failed retags are not skipped as already tagged), companion request validation, browser contract tests and a real-browser check of the action and its confirmation at phone to desktop widths in light and dark appearance. It was not run against a library with thousands of failed photographs.
- **Quiet automatic review** is covered by engine tests for new failures, notifications, the six-hour wait, explicit Retry and the clean-up of earlier failed automatic batches when a library opens.
- **Location history compaction** was trialled on a snapshot of a 93,806-photo catalog before this release: 231,961 of 1,146,609 history rows removed in about 10 s (longest step 607 ms), and reclaiming space took 13.7 s, shrinking the catalog from 4.15 GB to 3.07 GB with integrity checks passing. Undo of each source's latest relocation restored every photo on compacted and uncompacted copies.

## Limits

These changes were not measured on an 8 GB Mac, on macOS 15 or 26, or on another Mac. Household tagging is unchanged since build 142; two physical Macs helping each other were not tested. The Mac receipts above do not cover Linux or Windows.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset and signature verification, the live feed, installed public update/core/Stop/recovery, required source and churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
