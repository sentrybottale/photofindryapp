# PhotoFindry 0.2.0 beta 4 (build 84)

- More reliable portable source export, import and reconnection, preserving saved details and useful progress.
- Confirm matching originals without waiting for EXIF enrichment. Sources now separates connection readiness from camera and location scans, with progress, pause and recovery controls.
- Offline GPS-based place and proximity search, plus recorded capture-time filters. Approximate city labels come from bundled GeoNames data; no maps API or location upload is used. Location matches require saved GPS evidence, and metadata preferences control display and indexing.
- More reliable Local AI Search preparation, complete evidence ranking and recovery. Explicit user jobs take priority over background work.
- Optional diagnostic logs in the feedback form, shared only when you choose to include them. Email remains required for follow-up.

The update replaces the application bundle. Your photo library, original photographs, accepted tags and selected model paths are preserved; no retagging is required. Existing imported sources may need one explicit Review folder match confirmation before their originals can open. Cloud-only or unavailable files may still need to become locally available.

Free to use, with optional coffee support and no mandatory account or app tracking. Photo processing stays local; update checks, feedback submission and links are explicit network actions. This is an ad-hoc signed beta, not Apple Developer ID signed or notarized. Apple silicon and macOS 14 or newer are required; bundled Core ML semantic ranking requires macOS 15.

Guides: https://photofindry.com/docs/
Feedback: https://photofindry.com/bugs
