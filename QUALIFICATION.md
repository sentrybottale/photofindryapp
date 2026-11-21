# PhotoFindry 0.3.0-beta.10 / build 146 qualification

Mac build 146 was published on 21 November 2026. Linux keeps 0.3.0-beta.8 / build 142 and Windows keeps 0.3.0-beta.2 / build 132; their receipts are in the earlier releases.

## Changes

- **Smaller idle library worker.** The long-lived library worker now returns memory freed by background folder and face checks to macOS. With the published build 145 worker on a copy of a 93,806-photo library, with no browser, model or job, the worker's memory footprint grew to 1.22 GB within about a minute and stayed there; most of it was memory already freed but kept reserved for the worker. The same test with the build 146 worker held 0.08 GB (peak 0.20 GB) over the same 150 seconds, with the same CPU time. The memory safety budgets are unchanged; an 8 GB Mac budgets 1.5 GB for the worker.
- **Resume and Retry say why memory blocks them.** A refused Resume or Retry names the measurement that refused it: the process over its budget and that budget, PhotoFindry's combined budget, critical system memory pressure, or a measurement that is not available yet.
- **Browser opening after an update.** The local launcher submits PhotoFindry's fixed loopback address and returns once macOS has accepted it, so a default browser that is still starting no longer produces "The local browser opener did not respond". A rejected request still fails and is not opened twice.

No library migration, retagging or folder review is required.

## Exact candidate checks

The candidate was built and signed once from source revision c3e07ed. Every check below ran against those exact signed bytes.

- **Signed lifecycle:** 17 of 17 checks from the published build 145 to candidate 146: upgrade, rollback, anti-replay, signature rejection, interrupted switch recovery, serving policy and preserved-data uninstall/reinstall. The default browser was already running.
- **Real removable-volume churn:** 7 of 7, no skips (eject, flapping, forced eject, bulk deletion, stale mount point, same-named drive).
- **Source matrix (against the exact package):** 1,623 JavaScript tests with 1,597 passes, zero failures and 26 optional skips; 249 Python tests (two opt-in skips); 935 Rust tests with zero failures (18 ignored); and the Swift build. The first full run also passed with zero failures but skipped 43 native-parity suites, because this fresh checkout had not yet built their test driver; a second run stopped on one face-discovery test that steps the worker a fixed number of times, while a 27 GB copy ran beside it (it passed 5 of 5 alone). The final run had nothing else running.

The installed candidate ran the real-core checks on a 93,807-photo isolated library on an Apple M5 MacBook Pro with 32 GiB RAM and macOS 27.2.1, using Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8. This library was rebuilt for this release as a copy of the owner's current 93,806-photo library plus one local test photograph, so its counts differ from earlier releases' fixture:

| Check | Result |
| --- | --- |
| Ordinary search | 7,256 matches; 2.92 s |
| Completed scoped AI | 1 candidate; 21.6 s |
| Real tagging | 1 copied photograph; 32 observations; 26.8 s |
| Whole-pool retrieval to saved scores | 26,878 candidates; 26.5 s |
| Active Stop | 1.29 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

During these checks the build 146 library worker's memory footprint stayed between 0.02 and 0.29 GB while the vision model used up to 1.35 GB, and no memory safety pause occurred.

The Karate user-journey suite ran against the exact signed package on separate copies of the same library, and all 7 scenarios passed with no skips. The first run passed 4 scenarios. The 3 journeys that install the program stopped at the test harness's fixed 10-minute install limit, for the previous build 145 package as well as the candidate. Installing onto an existing library first saves and verifies a complete recovery copy, and this library is 27 GB: a direct install took 1,014 s. With the harness limit raised to 30 minutes, those 3 journeys passed on the same signed bytes.

| Journey | Result |
| --- | --- |
| Source remount (2 scenarios) | A cloud folder returned in 2.55 s after remount and a USB drive folder in 2.56 s after replug; a replaced original on the drive was flagged while the rest returned |
| Job control | Individual pause, queue pause, Quit and reopen, Resume and Stop of real tagging jobs |
| Install lifecycle | Prior release, signed update to the candidate in 12.8 s, uninstall and reinstall with library, jobs and model settings kept |
| Library from another Mac | 3,000 photos reconnected by relative location in 4.16 s reading 1.4 MB of 224.6 MB; after unplugging and replugging their drive, both folders returned in 5.63 s reading 1.36 MB |
| Tagging, search and AI | Real tagging 30.1 s; after a "man in suit" search, semantic coverage stayed at 70,090 photos; scoped AI 1 candidate; whole-library AI kept 26,878 candidates (floor 22,000); Stop 0.80 s |
| Showcase lens | Lens built in 50.3 s and saved; Local AI Search through it kept 36,561 candidates (6,248 in the lens) in 740 s; deleted; uninstall left the library unchanged |

These timings were taken on one Mac shared with the owner's normal PhotoFindry installation and other qualification work; they are not throughput claims.

## Limits

The memory change was measured on a 32 GB Mac. It was not run on an 8 GB Mac, so the release does not claim which memory limit refused AI Search there. It relies on a macOS memory allocator setting that the worker applies to itself at launch; if a future macOS ignores it, the worker behaves as in build 145. The full library state is still rebuilt about every two seconds while background folder checks change source status. The browser-opening change was covered by tests with a slow-starting test application, not by a cold start of a real browser. A fresh install onto an existing 27 GB library spends about 17 minutes saving and verifying its recovery copy before it opens. Not tested: macOS 15 or 26, another Mac, Linux or Windows.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset and signature verification, the live feed, installed public update/core/Stop/recovery, required source and churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
