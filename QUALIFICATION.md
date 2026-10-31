# PhotoFindry 0.3.0-beta.5 / build 138 qualification

This hotfix updates the Mac companion. Linux retains its signed 0.3.0-beta.3 / build 136 release and Windows retains 0.3.0-beta.2 / build 132. The existing beta 16 Mac-app migration channel is unchanged.

## Change

Build 137 still re-read originals in three cases: a library restored from another computer whose saved paths point elsewhere, a network mount that reports no volume identity, and choosing a folder, which read every photo once to record a checksum. Build 138 reconnects all three by each photo's relative location, size, modification time and change time, without reading or downloading original contents. A matching byte comparison now updates the saved proof, so the next return of that folder reads nothing. An original that cannot be read keeps its location and becomes available, still without a content read, once access returns. Background source checks process larger bounded steps and publish source/job notices at most every two seconds.

Ordinary search keeps its matches, order and Related coverage while doing less work: people-word searches reuse change tokens and coverage data, later Related pages reuse the first page's work, and stale saved projections are refreshed in grouped commits. A running Local AI Search is no longer stopped by background checks that only rewrite saved locations or re-encode unchanged saved text; changes to the photos in its scope still stop it. Refreshing an outdated saved projection keeps the saved vector of any unchanged evidence text; previously a people-word search re-projected such photos with empty vectors, removing them from Local AI Search retrieval until background indexing ran. The Karate user-journey suite found this; see Limits for the part that remains.

A file replaced at the same location still gets a new change time. It is compared against its saved checksum, or listed for review, and its saved tags are not transferred. No library migration, retagging or folder review is required.

## Exact candidate checks

The signed lifecycle passed all 17 checks from the published build 137 to candidate 138: upgrade, rollback, anti-replay, signature rejection, interrupted switch recovery, serving policy and preserved-data uninstall/reinstall. The complete Mac source matrix passed against the exact package: 1,616 JavaScript tests with 1,588 passes, zero failures and 28 optional skips; 248 Python tests (two opt-in skips); 864 Rust tests with zero failures (14 ignored); and the Swift build.

The installed candidate passed the real-core checks on a 93,807-photo isolated library on an Apple M5 MacBook Pro with 32 GiB RAM and macOS 27.0.1, using Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 6,883 matches; 3.42 s |
| Completed scoped AI | 1 candidate; 58.34 s |
| Real tagging | 1 copied photograph; 32 observations; 50.90 s |
| Whole-pool retrieval to saved scores | 22,191 candidates; 70.80 s |
| Active Stop | 1.81 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

These timings were taken while other qualification work shared the same Mac and are not throughput claims.

The Karate user-journey suite ran against the exact signed package on a separate copy of the same library: 5 of 6 scenarios passed (two source remount scenarios, job control, install/update/uninstall lifecycle and the imported library). Reconnecting 3,000 photos took 4.16 s and read 1.4 MB of 224.6 MB of original bytes. The tagging, search and AI scenario failed its whole-library floor: after a people-word search for a newly tagged photo, Local AI Search returned 21,168 candidates instead of at least 22,000 and correctly reported limited semantic coverage. Build 137 behaves the same way (21,043 before this build's change); see Limits.

## Limits

Provider-side substitutions that preserve size, modification time and change time are not detected without an explicit content comparison. A people-word ordinary search refreshes outdated saved projections. Photos indexed under an older evidence format change almost every line when refreshed, so they lose most saved vectors until background semantic indexing runs; while the queue is paused, Local AI Search reports limited semantic coverage and can omit them (on the isolated library, 1,016 such photos after one search). Resuming the queue re-indexes them. This is unchanged from build 137 and is planned for the next build. The isolated fixture's cloud folders are checked against live provider metadata; a full scan of every original is not claimed complete. Another Mac, Linux and Windows are not covered by these build 138 receipts.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset/signature verification, the live feed, installed public update/core/Stop/recovery, required source/churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
