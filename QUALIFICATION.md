# PhotoFindry 0.3.0-beta.7 / build 141 qualification

This Mac hotfix updates the Mac companion. Linux retains its signed 0.3.0-beta.3 / build 136 release and Windows retains 0.3.0-beta.2 / build 132. The existing beta 16 Mac-app migration channel is unchanged.

## Change

**Lens searches keep the typed words.** Local AI Search with a chosen Showcase lens used to search with the lens statement and write it into the search box in place of the owner's words. The workspace now sends the typed words with the lens. Retrieval uses those words, the lens grades the retrieved photos, and the reranker refines lens members against the words followed by the lens statement. The search box shows only the typed words. Without typed words a lens searches as in build 140, including every previewed photo in scope.

**Opened panel sections stay open.** Background source updates rebuild the Files panel; its collapsible sections were recreated closed. Panels now keep the sections the owner opened and the scroll position of a source refresh.

No library migration, retagging or folder review is required. The package file set is unchanged from build 140; `ShowcaseVision` is byte-identical.

## Exact candidate checks

The signed lifecycle passed all 17 checks from the published build 140 to candidate 141: upgrade, rollback, anti-replay, signature rejection, interrupted switch recovery, serving policy and preserved-data uninstall/reinstall. The final source revision rebuilt byte-identical signed archive and release envelope bytes, and the complete Mac source matrix passed against that exact package: 1,617 JavaScript tests with 1,589 passes, zero failures and 28 optional skips; 249 Python tests (two opt-in skips); 894 Rust tests with zero failures (15 ignored); and the Swift build.

The installed candidate passed the real-core checks on a 93,807-photo isolated library on an Apple M5 MacBook Pro with 32 GiB RAM and macOS 27.0.1, using Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 6,883 matches; 2.95 s |
| Completed scoped AI | 1 candidate; 40.97 s |
| Real tagging | 1 copied photograph; 32 observations; 34.27 s |
| Whole-pool retrieval to saved scores | 22,192 candidates; 44.42 s |
| Active Stop | 1.39 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

A separate run of the signed candidate worker on a copy of the same library searched through a saved Portrait lens with *portrait* typed beside it. The job kept *portrait* as its query and search-box text; the words retrieved 9,501 candidates (the lens-only whole-library search below has 30,807), 242 lens members came first and all 242 were refined, followed by 62 near and 1,057 other photos, in 89.3 s.

The Karate user-journey suite ran against the exact signed package on separate copies of the same library and passed all 7 scenarios with no skips:

| Journey | Result |
| --- | --- |
| Source remount (2 scenarios) | A cloud folder and a USB drive folder return by attributes after remount/replug with zero content reads; an original replaced elsewhere is compared and flagged |
| Job control | Individual pause, queue pause, Quit/reopen, Resume and Stop of real tagging jobs |
| Install lifecycle | Prior release, signed update to the candidate in 13.21 s, uninstall and reinstall with library, jobs and model settings kept |
| Library from another Mac | 3,000 photos reconnected by relative location in 4.13 s reading 1.4 MB of 224.6 MB; a later remount returned in 3.55 s |
| Tagging, search and AI | After a "man in suit" search, semantic coverage stayed at 65,189 photos and whole-library AI kept 22,083 candidates (floor 22,000); Stop 1.22 s |
| Showcase lens (no typed words) | Lens built from examples and words in 66.9 s; saved with one rule changed; Local AI Search through it kept 30,807 candidates with 2,002 lens members first and refined (331 s); deleted; uninstall left the library unchanged |

These timings were taken on one Mac while the owner's normal PhotoFindry installation and other qualification work shared it; they are not throughput claims.

## Limits

The opened-section fix was checked by a panel contract test that rebuilds the Files panel through background updates, and in a browser against a copy of the library; that browser session did not happen to receive a rebuilding update during the observation. Typed words beside a lens were checked on one lens and one query. Lens searches list photos without a local preview after lens results; preparing missing previews may download originals kept only in cloud storage. Lenses were not tested on macOS 15 or 26, on another Mac or with other tagging models. Linux and Windows are not covered by these build 141 receipts.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset/signature verification, the live feed, installed public update/core/Stop/recovery, required source/churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
