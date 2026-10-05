# PhotoFindry 0.2.0 beta 10 (build 93)

- Pause and Resume keep the same tagging job, saved progress and your Continue with available photos choice, including after reopening the app.
- Resume retries originals at their saved locations. Returning files can be verified against saved checksums without reconnecting the whole source. Moved folders and originals without enough identity evidence still need confirmation.
- Repeated tagging and source-location requests reuse unfinished work instead of creating duplicate jobs. Successful source checks return control to the intended jobs, while a later Pause still wins.
- Foreground work hands off correctly from background jobs. Memory and disk recovery recheck the relevant holds before processing resumes, and cancellation drains owned helpers.
- Tagging restarts retain foreground priority, and failed jobs keep an explicit scoped Retry action.

This update replaces the application only. Your library, original photographs, saved tags and selected model folders are preserved. It introduces no automatic retagging or source-data migration.

Free to use. Apple silicon and macOS 14 or later are required; bundled Core ML ranking requires macOS 15. This is an ad-hoc signed community beta, not Apple Developer ID signed or notarized.

Installation and help: https://photofindry.com/docs/
Feedback: https://photofindry.com/bugs
