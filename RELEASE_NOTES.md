# PhotoFindry 0.2.0 beta 8 (build 89)

- Verify original files now keeps its link to the paused tagging job through a source outage or a paused check. Reconnect the source and resume verification; successful verification continues the selected job from its saved progress. A later Pause or Stop of tagging still wins.
- Recovery also handles a check finishing before its reply arrives and choosing a replacement originals folder. Closing Sources no longer prevents a successful requested recovery from continuing.
- Deleted originals in a connected folder no longer prevent other available photographs from being processed. Saved tags and unresolved targets are retained for an explicit retry.
- Source review offers checksum verification when folder confirmation has no eligible matches. Cloud-only, changed and unavailable files remain unresolved.
- Updated the bundled database runtime to fix a concurrent database-opening/closing deadlock.

This update replaces the application only. Your library, original photographs, saved tags and selected model folders are preserved. No automatic retagging or database migration is introduced. An unavailable drive or cloud folder must be reconnected before verification can finish.

Free to use. Apple silicon and macOS 14 or later are required; bundled Core ML ranking requires macOS 15. This is an ad-hoc signed beta, not Apple Developer ID signed or notarized.

Installation and help: https://photofindry.com/docs/
Feedback: https://photofindry.com/bugs
