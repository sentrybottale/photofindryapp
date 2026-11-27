# PhotoFindry 0.3.0-beta.11 / build 149 qualification

Mac and Windows build 149 were published on 27 November 2026. Linux keeps 0.3.0-beta.8 / build 142; its receipts are in that release.

## Changes

- **Household tagging screens on Windows.** Windows builds 143 and 144 were built without the household extension: a test build rebuilt the companion program after the release build. Native tests are now built first and the release program last, and the build refuses a companion that does not contain the household screens.
- **Windows household folder.** Turning on helping from Settings failed on Windows with "Cannot create the private household settings folder": the companion created that folder with ordinary permissions, which the worker refuses. It is now created private to the user, as the worker requires.
- **Helpers behind a firewall connect out.** The library runs a relay while it has helpers or is pairing, and a helper keeps connections parked there. The library still makes the encrypted connection end to end with both computers' pinned keys; the relay holds no keys.
- **Recovery messages.** Package read errors name the file and the reason (for example that security software removed it); recovery reports both builds when neither verifies.

No library migration, retagging or folder review is required.

## Exact candidate checks

Both candidates were built and signed once from source revision 35d15a4 (Windows from 039b9c1, which changes only reviewed client hashes). Every check below ran against those exact signed bytes.

**Mac (Apple M5 MacBook Pro, 32 GiB, macOS 27.2.1)**

- **Signed lifecycle:** 17 of 17 checks from the published build 146 to candidate 149, with no browser running beforehand.
- **Real removable-volume churn:** 7 of 7, no skips.
- **Source matrix (against the exact package):** 1,623 JavaScript tests with 1,597 passes, zero failures and 26 optional skips; 261 Python tests (two opt-in skips); 939 Rust tests with zero failures (18 ignored); and the Swift build. Earlier attempts stopped on stale reviewed client hashes and on a missing debug test driver, and one run skipped 6 more suites because this checkout lacked two pinned test tools; the reported run had them.

The installed candidate ran the real-core checks on the same 93,807-photo isolated library as build 146, using Qwen3.5 4B Q4_K_M with its BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 7,256 matches; 4.83 s |
| Completed scoped AI | 1 candidate; 58.7 s |
| Real tagging | 1 copied photograph; 32 observations; 55.1 s |
| Whole-pool retrieval to saved scores | 26,878 candidates; 82.9 s |
| Active Stop | 1.3 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

Another application had a 27B language model loaded on the same Mac during these checks, which likely explains the slower AI timings than build 146 (21.6 s and 26.5 s).

The Karate user-journey suite ran against the exact signed package; all 7 scenarios passed with no skips. In the first run 6 passed: source remount (2), job control, install lifecycle, library from another Mac, and tagging, search and AI. The Showcase lens journey's search completed (36,561 candidates in 1,196 s), but the test runner then ran out of Java heap: it keeps every polled state for its report, and the longer search meant about 2,400 polls. With waits of 10 minutes or more polling every 2 seconds, the journey passed on the same bytes: lens built in 48.9 s, Local AI Search through it kept 36,561 candidates (7,182 in the lens) in 863 s.

**Windows (AMD Ryzen 5 5600G, Radeon RX 9060 XT 16 GB, Windows 11 build 26200)**

- **Native tests:** helper 11, platform 13, companion 52 passed.
- **Signed lifecycle:** 14 of 14 checks from a signed earlier build to candidate 149 in 62.7 s: install, autostart, sign-in task start, signature and checksum rejection, killed-update recovery (1.3 s), update, rollback, anti-replay, uninstall and reinstall with the library kept.
- **Installed real core** on a 19-photo library with Qwen3.5 0.8B over Vulkan, 7 of 7: ordinary search 6 matches; scoped AI; tagging 79 observations; whole-library AI exposed scored candidates and Stop took 1.2 s with progress kept; cancel all; recovery; uninstall with all jobs preserved.
- No Microsoft Defender detection during these runs.

**Household tagging: Mac 149 library, Windows 149 helper.** Fresh isolated libraries on both computers, 18 public test photographs and the same Qwen3.5 4B files on both. The PC connected out to the Mac's relay; both screens showed the same code and the pairing completed in 8.8 s. Each run accepted all 18 photographs:

| Run | Time | Photos per second |
| --- | --- | --- |
| Mac with the PC helping, first run (both loading models) | 335.0 s | 0.054 |
| Mac alone | 287.2 s | 0.063 |
| Mac with the PC helping | 97.6 s | 0.184 |

These timings were taken on the owner's own Mac and PC alongside other work; they are not throughput claims.

## Limits

Windows program files are not code-signed. On 17 November, during an earlier test, Microsoft Defender's behaviour detection removed an unsigned PhotoFindry program on the test PC; it raised no detection in these runs. Not tested: an 8 GB Mac, another Mac, macOS 15 or 26, other Windows PCs, Windows as the household library, a helper reached directly over the LAN rather than through the relay, and Windows People (not provided).

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset and signature verification, the live feeds, installed public update/core/Stop/recovery on Mac and Windows, required source and churn tests, the exact served installer commands and both websites. Completion is recorded only after those delivered-byte checks pass.
