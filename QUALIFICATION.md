# PhotoFindry 0.3.0-beta.6 / build 140 qualification

This release updates the Mac companion. Linux retains its signed 0.3.0-beta.3 / build 136 release and Windows retains 0.3.0-beta.2 / build 132. The existing beta 16 Mac-app migration channel is unchanged.

## Change

**Showcase lenses (Mac, macOS 15 or later).** The Lenses panel makes a lens from example photos and the owner's words. A separate job uses the configured tagging model once, sending only the saved descriptions of the examples, then measures the examples and a sample of up to 3,000 previewed library photos with the bundled `ShowcaseVision` helper to calibrate each rule. The owner reviews the rules, keeps or leaves out any of them and saves the lens. Local AI Search with a chosen lens lists lens members first, refined by the reranker, followed by every other candidate. Lenses are stored in the library, travel in portable backups and never change originals or tags. On macOS 14 the helper reports that lenses need macOS 15, and the rest of the program is unchanged.

**Local AI Search keeps every photo after a people-word search.** Builds 137 and 138 could leave about a thousand photos out of a whole-library AI search after an ordinary people-word search refreshed older saved text while the queue was paused. A refreshed line now keeps its saved vector, reuses one saved for the same text, or provisionally borrows the vector of the older line it restates until background indexing replaces it.

**Open PhotoFindry uses the sending browser.** The launcher passes the bundle identifier of the browser that sent the link; the opener accepts it only for an application macOS lists for local web pages and otherwise uses the default browser.

No library migration, retagging or folder review is required.

## Exact candidate checks

The signed lifecycle passed all 17 checks from the published build 138 to candidate 140: upgrade, rollback, anti-replay, signature rejection, interrupted switch recovery, serving policy and preserved-data uninstall/reinstall. The complete Mac source matrix passed against the exact package: 1,616 JavaScript tests with 1,588 passes, zero failures and 28 optional skips; 248 Python tests (two opt-in skips); 893 Rust tests with zero failures (15 ignored); and the Swift build. The final source revision rebuilt byte-identical signed archive and release envelope bytes to the candidate that ran the installed checks below.

The installed candidate passed the real-core checks on a 93,807-photo isolated library on an Apple M5 MacBook Pro with 32 GiB RAM and macOS 27.0.1, using Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 6,883 matches; 3.02 s |
| Completed scoped AI | 1 candidate; 26.11 s |
| Real tagging | 1 copied photograph; 32 observations; 31.92 s |
| Whole-pool retrieval to saved scores | 22,192 candidates; 39.40 s |
| Active Stop | 1.24 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

The Karate user-journey suite ran against the exact signed package on separate copies of the same library and passed all 7 scenarios with no skips:

| Journey | Result |
| --- | --- |
| Source remount (2 scenarios) | A cloud folder and a USB drive folder return by attributes after remount/replug with zero content reads; an original replaced elsewhere is compared and flagged |
| Job control | Individual pause, queue pause, Quit/reopen, Resume and Stop of real tagging jobs |
| Install lifecycle | Prior release, signed update to the candidate in 12.61 s, uninstall and reinstall with library, jobs and model settings kept |
| Library from another Mac | 3,000 photos reconnected by relative location in 4.14 s reading 1.4 MB of 224.6 MB; a later remount returned in 6.11 s; a replaced original shows changed |
| Tagging, search and AI | After a "man in suit" search, semantic coverage stayed at 65,189 photos and whole-library AI kept 22,083 candidates (floor 22,000); Stop 1.34 s |
| Showcase lens | Lens built from 4 examples and words in 62.3 s; saved with one rule changed; Local AI Search through it kept 30,807 candidates with 3,836 lens members first and refined; deleted; uninstall left the library unchanged |

These timings were taken on one Mac while other qualification work shared it and are not throughput claims.

## Limits

Showcase lenses judge previews and saved descriptions. Photos without a local preview are listed after lens results; in the lens journey 16,781 of 30,807 candidates had no preview, and that whole-library lens search took 475 s. Preparing missing previews may download originals kept only in cloud storage. Lens membership was compared with 172 photos the owner had labelled; it is not a guarantee of taste on other libraries. Lenses were not tested on macOS 15 or 26, on another Mac or with other tagging models. Provider-side substitutions that preserve size, modification time and change time are not detected without an explicit content comparison. Linux and Windows are not covered by these build 140 receipts.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset/signature verification, the live feed, installed public update/core/Stop/recovery, required source/churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
