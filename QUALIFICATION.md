# PhotoFindry 0.3.0-beta.8 / build 142 qualification

Mac build 142 was published on 8 November 2026 and Linux x86-64 build 142 on 9 November 2026. Windows retains 0.3.0-beta.2 / build 132 (see Windows below). The existing beta 16 Mac-app migration channel is unchanged.

## Changes

- **Household tagging (members).** Paired computers on the home network tag a library's reduced previews over a pinned, encrypted connection; originals, tags and the library stay on the library computer. Every computer taking part needs an active membership key. The key is checked when helping starts and again on each pairing and each lent batch, so a removed or lapsed key stops helping at the next batch.
- **Parallel tagging.** Check performance measures parallel slots on this Mac. New tagging jobs use more than one slot only when that was clearly faster with every sample accepted.
- **Exact vector counts.** The search index keeps per-photo vector counts in the writer's own transaction, so coverage no longer scans the vector table. An existing index is counted once in bounded, cancellable steps by the queued semantic job; coverage uses the old scan until then.
- **Lens builder.** Only measurable examples are offered, with a count of the others and an explicit Prepare previews action; builder searches have their own search lane so gallery refreshes do not cancel them.
- **Removed photos.** Finder metadata files no longer block emptying Removed.

No library migration, retagging or folder review is required.

## Exact candidate checks

The candidate was built and signed once. A rebuild from the final source revision produced byte-identical signed archive and release-envelope bytes, and the owner's Mac was updated from published build 141 to these exact bytes before release.

- **Signed lifecycle:** 17 of 17 checks from the published build 141 to candidate 142: upgrade, rollback, anti-replay, signature rejection, interrupted switch recovery, serving policy and preserved-data uninstall/reinstall.
- **Source matrix (final revision, against the exact package):** 1,619 JavaScript tests with 1,587 passes, zero failures and 32 optional skips; 249 Python tests (two opt-in skips); 925 Rust tests with zero failures (18 ignored); and the Swift build. Earlier attempts are kept on record: a client boundary test that did not yet allow the private extension module (fixed), one write-count failure in the private JavaScript comparison model that passed 6 of 6 times when re-run alone, a macOS disk-image creation that hung inside `hdiutil`, and a work-queue test whose two threads could deadlock in the test itself (made deterministic; 200 of 200 passes).
- **Real removable-volume churn:** 7 of 7, no skips (eject, flapping, forced eject, bulk deletion, stale mount point, same-named drive).

The installed candidate ran the real-core checks on a 93,807-photo isolated library on an Apple M5 MacBook Pro with 32 GiB RAM and macOS 27.2, using Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 6,883 matches; 2.72 s |
| Completed scoped AI | 1 candidate; 35.2 s |
| Real tagging | 1 copied photograph; 32 observations; 26.17 s |
| Whole-pool retrieval to saved scores | 22,192 candidates; 34.32 s |
| Active Stop | 1.22 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

The first core run passed every check up to recovery, then its `lifecycle recover` call exceeded the runner's 180-second limit (the product allows 390 seconds). The cause was not identified. Two further complete runs passed, including one on a copy of the library with the new vector-count tables removed, which reproduces a library opened by build 141.

The Karate user-journey suite ran against the exact signed package on separate copies of the same library and passed all 7 scenarios with no skips (3,360 s):

| Journey | Result |
| --- | --- |
| Source remount (2 scenarios) | A cloud folder and a USB drive folder return by attributes after remount and replug; 3,000 photos returned in 4.10 s reading 1.35 MB |
| Job control | Individual pause, queue pause, Quit and reopen, Resume and Stop of real tagging jobs |
| Install lifecycle | Prior release, signed update to the candidate in 12.5 s, uninstall and reinstall with library, jobs and model settings kept |
| Library from another Mac | 3,000 photos reconnected by relative location in 4.15 s reading 1.4 MB of 224.6 MB |
| Tagging, search and AI | After a "man in suit" search, semantic coverage stayed at 65,189 photos and whole-library AI kept 22,083 candidates (floor 22,000); Stop 1.16 s |
| Showcase lens | Lens built and saved; Local AI Search through it kept 30,807 candidates with the lens members first and refined; deleted; uninstall left the library unchanged |

These timings were taken on one Mac shared with the owner's normal PhotoFindry installation and other qualification work; they are not throughput claims.

## Household tagging scope

Pairing, membership gates and helper refusal are covered by engine and companion tests. Two isolated companions on one Mac were driven in a browser: neither could pair or lend without a key, the helper refused even with the Settings switch bypassed, both paired with keys, removing the helper's key stopped its helper process, and re-entering it resumed helping. Earlier private trials ran a helper on a separate Windows PC over the home network; the public Windows release does not include household tagging yet. Two physical Macs helping each other on build 142 were not tested. The online membership confirmation list at app.photofindry.com was published after the Mac release on 8 November 2026, and an installed build 142 confirmed its key against it.

## Limits

Parallel tagging and exact vector counts were not measured on an 8 GB Mac. Lenses were not tested on macOS 15 or 26, on another Mac or with other tagging models. The Mac receipts above do not cover Linux or Windows; see their sections.

## Linux x86-64

The Linux companion was built once from the same household revision on an Intel N100 PC with 8 GB RAM running Linux Mint 21.3 (Ubuntu 22.04 base, kernel 6.8), then signed with the same publisher key. The first package carried third-party notices that predated nine newly added Rust crates; the notices were regenerated and the package rebuilt from the identical binaries before any test below.

- **Signed lifecycle:** 13 of 13 from published build 136 to candidate 142: install, autostart, signature and checksum rejection, interrupted-update recovery, update in 12.1 s, rollback, older-build refusal and preserved-data uninstall/reinstall.
- **Unit tests on the same machine:** companion 107 passed (1 ignored); worker household, membership, search-index and tagging tests 30 passed (2 ignored); zero failures.
- **Installed real core** on a 19-photo isolated library with Qwen3.5 4B Q4_K_M on CPU: ordinary search 6 matches in 0.06 s; scoped AI 32.2 s; tagging 61 observations in 370.2 s; whole-library AI 19 candidates in 46.5 s, Stop 3.15 s; cancel all, recovery and uninstall kept the library and all 42 prior jobs.

Household helping between a Mac and a Linux computer was not tested. GPU acceleration, other distributions and desktop environments are not covered.

## Windows

Windows stays on 0.3.0-beta.2 / build 132. Windows build 142 was built twice on a Ryzen 5 5600G PC with an RX 9060 XT and 16 GB RAM, and neither build was published:

- The first build passed its native tests and the signed 132 → 142 lifecycle (13 of 13) on 8 November 2026. Its first real-model run paused for memory safety while a game was running; that pause message wrongly says "this Mac".
- On 9 November, Windows 11 Smart App Control on that PC blocked fresh copies of the unsigned build 142 signature verifier and of the bundled llama.cpp runtime library (the same bytes as build 132). A repeat lifecycle run failed at interrupted-update recovery, and the real-model run failed at Local AI Search.
- A second build 142 starts each new program once before switching versions, so a refused update keeps the working version. It passed its native tests (11, 13 and 51 passed, 1 ignored) with Smart App Control off. Its lifecycle failed at interrupted-update recovery: Microsoft Defender reported Behavior:Win32/Persistence.A!ml when recovery re-registered the per-user logon task for the prior version, quarantined that program file and removed the test task. A repeat run was refused before it started because Defender then blocked a fresh copy of the build 132 program inside the test harness. The owner's existing build 132 installation kept running.

Existing build 132 installations keep working on that PC. Unsigned Windows program files can be blocked by Smart App Control or flagged by Defender's behaviour monitoring. Windows will move only after a build whose updates no longer rewrite the logon task passes these checks.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset and signature verification, the live feed, installed public update/core/Stop/recovery, required source and churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
