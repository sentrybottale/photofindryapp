# PhotoFindry 0.3.0-beta.3 / build 136 qualification

This release updates Mac and Linux. Windows retains its separately signed 0.3.0-beta.2 / build 132 release. The existing beta 16 Mac-app migration channel is unchanged.

## Exact candidate checks

The Mac signed lifecycle passed 17 checks and Linux passed 13: upgrade, rollback, signature rejection, interruption, recovery and preserved-data uninstall/reinstall. The complete Mac source matrix passed: 1,585 JavaScript tests, zero failures (31 optional skips), Python, Rust and Swift. Required delivered-source checks are separate from optional source experiments.

Both installed candidates passed all seven real-core checks. Mac reused a 93,807-photo isolated library on an Apple M1 with 8 GiB RAM and macOS 27.0.1. Linux used the retained 19-photo real-evidence fixture on the headless GE Intel N100 CPU host with about 7.5 GiB usable RAM. No routine full-library export/import or index rebuild was used.

| Check | Mac | Linux |
| --- | --- | --- |
| Ordinary search | 16,396 matches; 14.83 s | 6 matches; 0.07 s |
| Completed scoped AI | 1 candidate; 82.24 s | 1 candidate; 30.16 s |
| Real tagging | 1 copied photograph; 23 observations; 229.10 s | 1 copied photograph; 61 observations; 371.07 s |
| Whole-pool retrieval through saved scores | 25,394 candidates; 74.46 s | 19 candidates; 46.53 s |
| Active Stop | 1.85 s; saved results retained | 4.12 s; saved results retained |

Mac tagging included the first automatic performance check for the verified model copies: four profiles were compared and the baseline was retained. Its measured baseline was about 42.7 seconds per photograph; that is a local sample, not a sustained-throughput claim. Mac used Qwen3.5 4B Q4_K_M with its matching BF16 projector and Qwen3 reranker 0.6B Q8. Linux used its configured Qwen3.5 0.8B CPU model. Quality and memory checks were retained.

The actual interrupted Stop from the failed trial reopened as cancelled on build 136, with its input, checkpoint and targets unchanged. The corresponding regression also preserves individual pauses across interrupted shutdown. Linux additionally passed seven authenticated browser-maintenance checks, including killing a live rollback requester while the owned maintenance continued.

## Normal Mac and limits

The normal Mac upgrade retained all 93,806 photo IDs, all 101 prior jobs and their pause/cancel states, the complete header/model settings and the exact retained native folder-access owner. Work reconnected and background checking continued while the queue stayed paused. Its full original-file scan is not claimed complete; missing or changed files still need their specific recovery action.

The first ordinary people search on this normal library spent 122.28 seconds validating saved evidence after the upgrade; repeating it returned the same 16,396 matches in 0.86 seconds. Validation is retained for unchanged evidence. Broad first searches can still take seconds; these results do not claim every search is instantaneous. Existing cached preview bytes and full details were checked separately.

Initial model access in Downloads stalled before inference. The core trial used full-file-hash-verified copies. On the normal installation, native selection of the same original model, projector and reranker paths followed by reopening restored access; the original model loaded in 11.56 seconds. No model files were replaced or moved, and no selected paths were changed. macOS access cannot be inferred from a unit test or a successful read in Terminal.

A diagnostic Recover issued before Quit finished did not reopen the normal service within the caller's 180-second budget. Starting the existing launch agent after it stopped reopened the same library. This attempt is retained separately from the completed signed lifecycle/core recovery checks; no library backup was restored.

The clean MIT client export contains 168 allowlisted files plus inventory. Proprietary source/history, inference policies, user data and private traces are excluded. Required public build, boundary and security checks remain enforced. Automated scans and signatures do not establish absence of bugs or an exhaustive independent audit.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset/signature verification, the live platform feeds, installed public update/core/Stop/recovery, required source/churn tests, the exact served installer commands and both websites. Completion is recorded only after those delivered-byte checks pass.

Linux desktop GUI acceptance, another Mac, every OS/codec/GPU/cloud condition and completed refinement of the entire Mac candidate pool are not claimed. Windows is unchanged and has no People provider. Dedicated CLEF/Intern reviewers and scoped source archives remain Mac-only.
