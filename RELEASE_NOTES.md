# PhotoFindry 0.2.0 beta 5 (build 85)

- Ignore location, place selection and radius changes keep Local AI Search active. The previous search keeps its saved results while a new search uses the changed constraints.
- Live refinement no longer presents a resumable checkpoint as a paused search between scoring batches.
- Typing keeps the search cursor when progress, location or diagnostic controls change height. Gallery keyboard shortcuts respect native text editing.
- Legacy saved text no longer causes “Invalid saved photograph” in direct or metadata-filtered related search. Stored records are preserved; no retagging or migration is needed.

The update replaces the application bundle. Your photo library, original photographs, accepted tags and selected model paths are preserved.

Free to use, with optional coffee support and no mandatory account or app tracking. Photo processing stays local; update checks, feedback submission and links are explicit network actions. This is an ad-hoc signed beta, not Apple Developer ID signed or notarized. Apple silicon and macOS 14 or newer are required; bundled Core ML semantic ranking requires macOS 15.

Guides: https://photofindry.com/docs/
Feedback: https://photofindry.com/bugs
