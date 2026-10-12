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
