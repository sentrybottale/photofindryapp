# PhotoFindry 0.2.0-beta.14 — the browser client

PhotoFindry now opens its library in a locally served browser workspace, with a
small native Mac launcher and authenticated companion. Start from
app.photofindry.com or the installed PhotoFindry application. Photos, models,
tagging, people and search stay on your Mac; the public website receives no local
library session or engine authority.

The client is open source under MIT: Svelte UI, native launcher, generic Rust
companion bridge and selected Mac interaction helpers. The Rust engine cores
remain proprietary. Official builds remain free to use.

- Existing-library migration preserves saved evidence, names, edits, model paths
  and paused work. A verified recovery copy and Use current beta alternative help
  you recover from a failed transition without silently restoring older data.
- Responsive navigation, richer photo details, Light/Dark/Follow System, model
  and token monitoring, durable Jobs and source-location recovery are available
  in the browser workspace.
- Portable backups and source archives remain compatible. After importing from
  another Mac, confirm where the original folders live on this Mac.
- Uninstall removes the verified application through its local confirmation flow
  while preserving libraries, originals, model files and backups.

Requires Apple silicon and macOS 14 or newer; Core ML semantic ranking requires
macOS 15. Vision model weights remain separate user-selected files. This is a
community beta with ad-hoc macOS bundle signatures and separately authenticated
release/update artifacts; it is not Apple Developer ID signed or notarized.

Keep independent backups. Release verification authenticates exact bytes and the
signing identity; it is not a claim of flawless security or an OS sandbox.

---

# PhotoFindry 0.2.0 beta 13 (build 108)

- Review originals checks saved locations and file attributes without reading every original's contents. Explicit folder confirmation gives blocked tagging a direct recovery action while preserving saved photo identities and historical checksums. Full content comparison remains a separate action.
- Tagging resumes more reliably after a source is remounted or confirmed, skips repeated access checks for unconfirmed originals, and continues through individual unavailable files while retaining them for explicit retry.
- Camera, lens and location filters suggest values from saved library metadata. Camera families match normalized recorded models; actual focal-length choices stay distinct from lens names. Suggestions do not start a model or read original images.
- Current validated photo-review answers refine the saved search projection. Contradicted or uncertain model claims lose direct support; supported follow-ups appear as labeled review suggestions. Accepted tags, human labels and original review history remain intact, and Undo restores the prior projection.
- Search better recognizes saved apparent age ranges without retagging. Estimates remain uncertain observations, not verified identity or exact age.
- Interrupted library cleanup and deletion retain recovery progress, including after clearing Jobs. Activity errors stay scoped to their jobs.

Updates replace the application bundle and preserve your library, originals, accepted tags, names and selected model paths. No automatic retagging or source-data cleanup is introduced. Search indexes remain rebuildable derivatives of saved evidence.

Free to use. Apple silicon and macOS 14 or later are required; bundled Core ML ranking requires macOS 15. This community beta uses ad-hoc bundle signatures and independently signed release/update artifacts. It is not Apple Developer ID signed or notarized.

Installation and help: https://photofindry.com/docs/
Feedback: https://photofindry.com/bugs
