# PhotoFindry 0.2.0 beta 12 (build 107)

- Photo queues continue across individual original-file failures while keeping unavailable targets for explicit retry. Decision review recovers from expired local connections and safely reuses its owned model between photos.
- Photo detail review has independent model settings, an optional Intern-Decision 4B reviewer and experimental CLEF Flash support. Experimental answers are suggestions, not automatic changes to accepted facts or people names. Model weights remain separately downloaded or selected in your own folders.
- Intern review includes corrected ternary scoring and an explicit low-memory trial. Actual 8 GB hardware qualification remains limited; memory protection still applies.
- Improvement activity shows the current prepared photo, real progress and last committed review. The inspector shows the saved detail behind suggestions, and Recently improved orders by committed review time.
- The model sidebar follows the actual loaded model and task. Activity reports measured usage across tagging, search and decision review, preserving session totals across model restarts. Unload pauses its model task and keeps work for Resume.

Updates replace the application bundle and preserve your library, originals, accepted tags, names and selected model paths. No automatic retagging or source-data cleanup is introduced.

Free to use. Apple silicon and macOS 14 or later are required; bundled Core ML ranking requires macOS 15. This community beta uses ad-hoc bundle signatures and independently signed release/update artifacts. It is not Apple Developer ID signed or notarized.

Installation and help: https://photofindry.com/docs/
Feedback: https://photofindry.com/bugs
