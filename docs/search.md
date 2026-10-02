# Search your photographs

PhotoFindry offers ordinary search and an explicit Local AI Search action. Both use your selected library or folder scope; the evidence available depends on what has been saved.

## Ordinary search

Type a filename or describe the photograph in the search field. Filename search works before tagging. Saved descriptions and details become searchable as tagging completes. Ordinary search does not start a large language model; optional local semantic ranking may reorder related matches.

Check the current folder, status filters and metadata filters if a result seems missing. A filename match is a filename match, not proof that the image shows the words in the filename. Literal names and quoted terms should not be silently replaced by spelling suggestions.

## People descriptions

Beta 9 understands ordinary wording such as **boy in purple** when the saved details support that person's apparent age, appearance and clothing. Existing compatible tags can be used without retagging. These are model appearance estimates, not verified identity, gender or exact age; uncertain descriptions may not support a match. Quoted terms keep their literal meaning.

New tagging preserves supported plain descriptions from your configured local model. Review saved details and correct mistakes rather than assuming every description is accurate.

## Local AI Search

Choose Local AI Search explicitly for a closer review of saved photo evidence. Typing by itself does not start this job. It runs locally with configured models and keeps its selected scope.

Ranked candidates are leads, not confirmed matches or probabilities. Best so far is a limited view; Refined and All candidates let you inspect more of the retained candidate pool. Review the supporting photo details before relying on a result. Incomplete tagging or semantic indexing means incomplete evidence coverage.

## Pause and Stop

Pause retains continuation. Stop cancels the remaining AI work and drains its owned model while keeping reviewed matches. A stopped search is not resumable; start a new search when ready. Results saved earlier can become stale after library facts change.

If a search appears empty, first review filters and whether saved evidence is ready. Do not retag an entire library as the first troubleshooting step. See [Troubleshooting](https://photofindry.com/docs/troubleshooting.html).

## Offline GPS and capture time

Beta 9 can use recorded GPS coordinates for place and proximity searches, including Local AI Search. For example, search for Milan or combine a place with a time-of-day description. Nearby-city labels are approximate GeoNames labels, not exact addresses. No maps API or coordinate upload is used. A visual resemblance to Italy is not proof that a photo was taken there.

Open a photograph's Camera & location section to see its saved metadata. In Sources, camera and location scanning is separate from confirming originals. Scan or resume the source there if GPS has not been read yet. Photos without recorded GPS cannot gain a verified location from this scan. Recorded capture time is used as stored; unknown time zones remain unknown. Metadata preferences control display and search indexing.

## Changing location during AI Search

Beta 9 keeps Local AI Search active when you choose Ignore location, another place or another radius. The old search keeps its saved results; a fresh search uses the changed GPS constraints. Typing a different query returns to ordinary search until you explicitly choose Local AI Search again. A live saved checkpoint does not mean refinement is paused.
