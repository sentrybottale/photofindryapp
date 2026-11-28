# PhotoFindry 0.3.0-beta.13 / build 151 qualification

Mac build 151 was published on 28 November 2026. Windows keeps 0.3.0-beta.11 / build 149 and Linux keeps 0.3.0-beta.8 / build 142; their receipts are in those releases. Build 150 was a private test build of the source-removal fix and was not published.

## Changes

- **Source removal.** Removing a source failed on a 93,806-photo library with "Source People metadata exceeds its storage bound.": the archived removal capped the People header at 16 MiB while the People store allows 128 MiB, and it reset People revision counters. People rewrites for source removal and permanent deletion now use the store's own 128 MiB bound and its separate counters row. **Remove source…** asks whether to keep an archive. Without one, a final irreversible confirmation queues one journaled removal job that shares the permanent-deletion journal, Jobs recovery and backup blocking with Empty Removed. The source is held inactive before rows are deleted, so discovery cannot add its files back, and each store is cleaned once per source. Permanent removal without an archive needs no Library cleanup review or completed source audit; previously, permanent deletion was only available for reviewed missing entries through Empty Removed. Original photographs are never touched.
- **Household batches.** A household batch can now be up to the rest of a 32-photo turn instead of a few photos, so computers wait for each other much less often; source checks can still end a batch earlier. Helper connections open in the background while this computer's model starts, join the running batch when ready and last for the turn. Once a helper's pace is measured in the job, it rests when its pace times the others' combined rate exceeds the photos left; this computer never rests. A helper turned off gets no new photo after about two seconds; a helper whose connection or model fails, or whose photo passes the 180 s response limit, gets no more photos in that batch and stops counting as capacity, and its photo is tagged on this computer. The protocol is unchanged, so build 149 helpers keep working.
- **Per-computer activity.** While a tagging job runs with household helpers, the tagging strip shows one row for this computer and one for each helper (up to eight): its state or why it is not helping, its photos per minute once measured and its tagged and failed counts for that job. Rows use fixed states and reason codes, plus each helper's computer name as recorded at pairing (control characters removed, at most 64 characters); helper error text, addresses and keys are never shown.
- **Review answers.** With the default Current tagging model reviewer, wording that visual verification cannot establish (people counts, names, safety ratings, exact ages, or whole-scene details from a crop) gets the usual one correction. If it remains, it is left out and that check is recorded as unclear; its other valid observations can still be added as review details when the question was not a targeted check, and such an answer never marks an existing saved detail as uncertain or conflicting. A review answer that is still rejected keeps saved tags, its text and its automatic hold, but is no longer a job failure, a Needs attention item or a photo-detail warning. Existing saved rejections with that wording are hidden when read; older or other improvement issues still show, and no saved data is rewritten.

No library migration, retagging or folder review is required.

## Exact candidate checks

The candidate was built and signed once from source revision 223f277. Every check below ran against those exact signed bytes on an Apple M5 MacBook Pro (32 GiB, macOS 27.2.1).

- **Signed lifecycle:** 17 of 17 checks from the published build 149 to candidate 151.
- **Real removable-volume churn:** 7 of 7, no skips.
- **Source matrix (against the exact package):** 1,631 JavaScript tests with 1,605 passes, zero failures and the same 26 optional skips as build 149; 262 Python tests (two opt-in skips); 961 Rust tests with zero failures (18 ignored); and the Swift build.

The installed candidate ran the real-core checks on the same 93,807-photo isolated library as build 149, using Qwen3.5 4B Q4_K_M with its BF16 projector and Qwen3 reranker 0.6B Q8:

| Check | Result |
| --- | --- |
| Ordinary search | 7,256 matches; 2.92 s |
| Completed scoped AI | 1 candidate; 21.5 s |
| Real tagging | 1 copied photograph; 32 observations; 25.4 s |
| Whole-pool retrieval to saved scores | 26,878 candidates; 26.9 s |
| Active Stop | 1.57 s; saved results retained |
| Recovery, retained folder-access owner, uninstall | Completed results reopened; 93,807 photos and all 73 jobs preserved |

The Karate user-journey suite ran against the exact signed package; all 7 scenarios passed with no skips, over three runs on the same bytes. The first run passed 4: source remount (2; 3,000 photos reconnected in 4.16 s while reading 1.4 MB of 225 MB of originals), job control, and the library from another Mac. The 3 installed journeys were refused before starting because the disk lacked the 31 GB the fixture's verified recovery copy needs. After space was freed, install lifecycle (signed update in 12.7 s) and tagging, search and AI (tagging 67.5 s, ordinary search 8,969 matches in 14.9 s, whole-library AI 26,878 candidates with Stop in 0.89 s) passed, measured while the household test below also ran on this Mac. The Showcase lens journey's build then measured none of its example photos: the test library, cloned from a live library, still pointed at preview files there that a restore had moved. Its preview paths were pointed at its own copies, and the lens journey passed: the lens built in 51.8 s and a Local AI Search through it kept 36,561 candidates, 3,704 of them in the lens, in 514 s.

**Household tagging: Mac 151 library, Windows 149 helper.** A fresh isolated library on this Mac (18 public test photographs, Qwen3.5 4B) paired with a Windows PC running the published build 149 as its normal installation, whose helping was already on; it paired directly over the LAN in 6.3 s after both computers showed the same code. Each run accepted all 18 photographs, and the tagging strip showed a row for this computer and for the PC (opening, working and idle states):

| Run | Time | Photos per second | Photos tagged by the PC |
| --- | --- | --- | --- |
| Mac with the PC helping, first run (PC loading its model) | 385.5 s | 0.047 | 5 |
| Mac alone | 293.1 s | 0.061 | 0 |
| Mac with the PC helping | 208.4 s | 0.086 | 6 |

These runs shared this Mac with the Karate journeys above, so they are slower than an idle Mac and are not throughput claims. The trial pairing was removed from the PC afterwards and no process was left on either computer.

**Source removal (measured on private build 150, the same removal code).** On a copy of a 93,807-photo library: Archive, then remove of a 1-photo source finished in 118 s; Remove without archive of a 60,978-photo source finished in 8 min 1 s, clearing 31,819 unshared cached copies (3.31 GB) and keeping the other source's 32,827 photos; every file still referenced afterwards existed. The People header shrank from 112.5 to 36.2 MiB as that source's faces left.

## Limits

Not tested: an 8 GB Mac, another Mac, macOS 15 or 26, three or more computers in one household batch (the resting and turned-off rules were checked by automated tests), a helper turned off during a real batch, and Windows or Linux as the library (they keep builds 149 and 142). Remove without archive is Mac only.

## Public delivery

Candidate results do not qualify public downloads. Publication requires fresh anonymous asset and signature verification, the live feed, installed public update/core/Stop/recovery, required source and churn tests, the exact served installer command and both websites. Completion is recorded only after those delivered-byte checks pass.
