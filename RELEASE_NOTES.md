# PhotoFindry 0.3.0-beta.7 · build 141

This Mac hotfix keeps your typed words when you search through a Showcase lens, and keeps opened panel sections open. Linux keeps 0.3.0-beta.3 / build 136 and Windows keeps 0.3.0-beta.2 / build 132.

**Lens searches keep your words.** Choosing a lens beside Local AI Search used to replace the words in the search box with the lens's own description and search with that instead. Your words are now the search: they decide which photos to look at, the lens grades those photos, and the reranker orders the photos in the lens by your words and the lens together. The search box keeps showing what you typed. With an empty search box a lens behaves as before and looks at every photo with a preview.

**Opened sections stay open.** While a source was being checked in the background, the Files panel rebuilt itself on every update and closed sections such as Source options a moment after they were opened. Panels now remember the sections you open and keep your scroll position when they refresh.

Updates preserve the library, originals, model paths, saved lenses and saved work. All inference remains local. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
