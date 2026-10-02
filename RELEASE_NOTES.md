# PhotoFindry 0.2.0 beta 9 (build 91)

- Ordinary searches such as "boy in purple" now find supported descriptions of the same person in existing saved tags. Boy, girl, man and woman wording uses the model's apparent age and appearance estimates; it does not verify identity, gender or exact age. No retagging is required.
- New tagging preserves the configured local model's supported plain descriptions, including ordinary people labels and factual sensitive-scene observations. Unsupported details still remain uncertain.
- Empty and corrupt original files no longer stop other usable photos from being tagged. Pending targets and saved tags are retained; reconnect originals and explicitly Resume or Retry to recover them.
- Failed tagging jobs now appear in Queue with a scoped Retry action ahead of scheduled or source-held work.

This update replaces the application only. Your library, original photographs, saved tags and selected model folders are preserved. No automatic retagging or source-data migration is introduced. Search indexes may refresh from saved observations.

Free to use. Apple silicon and macOS 14 or later are required; bundled Core ML ranking requires macOS 15. This is an ad-hoc signed beta, not Apple Developer ID signed or notarized.

Installation and help: https://photofindry.com/docs/
Feedback: https://photofindry.com/bugs
