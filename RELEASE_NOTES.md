# PhotoFindry 0.3.0-beta.6 · build 140

This release updates the Mac companion. Linux keeps 0.3.0-beta.3 / build 136 and Windows keeps 0.3.0-beta.2 / build 132.

**Showcase lenses (Mac, macOS 15 or later).** A lens is your own way of seeing the library. Open Lenses beside Local AI Search, find a few related photos with the ordinary search, tick the ones you are proud of, write what makes them your best work and choose Generate rules. A job uses your selected tagging model once — only the saved descriptions of your examples are sent to it — then measures the examples and a sample of your library to calibrate each rule. Review the rules, keep or leave out any of them, and save the lens. Choose it beside Local AI Search: the lens decides which photos belong, the reranker orders them by how well their saved descriptions fit your words, and every other candidate stays listed after them. The lens card above the results shows its rules and lets you search again, prepare missing previews, export the photos as a website section or delete the lens. Lenses are saved in the library and travel in library backups; they never change originals or tags.

**Local AI Search keeps every photo after a people-word search.** Build 138 could leave about a thousand photos out of a whole-library AI search after an ordinary search for words such as man or woman refreshed older saved text while the queue was paused. A refreshed line now keeps its saved vector, reuses one saved for the same text, or briefly borrows the vector of the older line it restates until background indexing replaces it, so the candidate pool stays complete.

**Open PhotoFindry uses the browser you clicked it in.** The workspace previously always opened in the system default browser; it now opens in the browser that sent the link when macOS lists it as able to open local pages, and otherwise in the default.

Updates preserve the library, originals, model paths and saved work. All inference remains local. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
