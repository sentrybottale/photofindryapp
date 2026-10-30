# PhotoFindry 0.3.0-beta.4 / build 137 qualification

This hotfix updates the Mac companion. Linux retains its signed 0.3.0-beta.3 / build 136 release and Windows retains 0.3.0-beta.2 / build 132. The existing beta 16 Mac-app migration channel is unchanged.

## Change

Build 136 treated a cloud-drive remount (for example pCloud, which assigns new device and inode numbers on every mount) as a request to SHA-256-read every original, and saved that mode on the folder. The same-volume re-plug of an external drive also forced a whole-folder byte check. Build 137 reconnects these folders by each file's size, modification time and change time. A replaced file receives a new change time and is still compared against its saved checksum; a mismatch is reported as changed and its saved tags are not transferred. The saved byte-only mode is removed from existing folders automatically. No library migration, retagging or folder review is required.

## Exact candidate checks

The signed lifecycle passed all 17 checks from the published build 136 to candidate 137: upgrade, rollback, anti-replay, signature rejection, interrupted switch recovery, serving policy and preserved-data uninstall/reinstall. The complete Mac source matrix passed against the exact package: 1,616 JavaScript tests with 1,588 passes, zero failures and 28 optional skips; 247 Python tests (two opt-in skips); 851 Rust tests with zero failures (14 ignored); and the Swift build. The first matrix attempt failed one unchanged companion timing test (`dropped_request_stops_only_its_owned_helper`, a 400 ms helper-start budget under full load); it passed alone three times and the complete matrix then passed. The failed attempt is retained.

The installed candidate passed the real-core checks on a 93,807-photo isolated library on an Apple M5 MacBook Pro with 32 GiB RAM and macOS 27.0.1, using Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 6,883 matches; 2.84 s first run, 0.24 s repeated |
| Completed scoped AI | 1 candidate; 19.21 s |
| Real tagging | 1 copied photograph; 32 observations; 30.17 s |
| Whole-pool retrieval to saved scores | 22,192 candidates; 52.31 s |
| Active Stop | 1.51 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all unfinished jobs preserved |

The first core attempt failed: Local AI Search started during the first reconnect pass of the fixture's network-drive folder and stopped with "The library changed during search". That pass also corrects stale saved folder paths once, which fences searches already in flight. A steady-state check then showed no further search-generation changes while source checks continued, and a fresh attempt passed every check. The failed attempt is retained; no assertion, threshold or timeout was changed.

These measurements come from a faster Mac than build 136's M1 / 8 GiB qualification and are not comparable throughput claims.

## Limits

The isolated fixture's cloud folders are checked against live provider metadata; a full scan of every original is not claimed complete. Provider-side substitutions that preserve size, modification time and change time are not detected without an explicit content comparison. Another Mac, Linux and Windows are not covered by these build 137 receipts.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset/signature verification, the live feed, installed public update/core/Stop/recovery, required source/churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
