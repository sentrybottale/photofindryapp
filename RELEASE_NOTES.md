# PhotoFindry 0.3.0-beta.5 · build 138

This hotfix updates the Mac companion. Linux keeps 0.3.0-beta.3 / build 136 and Windows keeps 0.3.0-beta.2 / build 132.

Reconnecting originals is fast again. A library restored from another computer, or a folder chosen at a new location, reconnects each photo by its relative location, size and dates, without reading or downloading the original. Choosing a folder no longer reads every photo once to record a checksum. Background checks handle many more photos per step, so large cloud folders finish in minutes instead of hours.

Ordinary search is faster without changing its results. Searches for people words such as woman or girl return in roughly a third to half of the previous time, later pages of Related results load almost immediately instead of repeating the whole search, and refreshing stale saved projections after an upgrade commits in groups. Matches, order and Related coverage are unchanged. A running search also keeps going while background checks reconnect originals, unless the photos in its scope or their saved details actually change. Refreshing an outdated saved search entry now keeps the semantic index of every unchanged line. Known limit, unchanged from build 137: photos indexed under an older evidence format still lose most of their semantic index when an ordinary search for words such as man or woman refreshes them, and while the queue is paused Local AI Search reports limited coverage and can omit them until the queue is resumed. A fix is planned for the next build.

A file replaced at the same location still gets a new change time: it is compared against its saved checksum, or listed for review, and its saved tags are not transferred. No library migration, retagging or extra folder review is required.

Updates preserve the library, originals, model paths and saved work. All inference remains local. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
