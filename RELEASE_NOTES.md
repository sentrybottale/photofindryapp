# PhotoFindry 0.3.0-beta.8 · build 142

This release adds household tagging for members, measured parallel tagging, faster related-search coverage and lens-builder fixes. Mac build 142 was published on 8 November 2026; Linux x86-64 build 142 followed on 9 November 2026. Windows keeps 0.3.0-beta.2 / build 132: in testing, Windows 11 Smart App Control blocked parts of the unsigned build 142, and Microsoft Defender flagged its update recovery, so it was not published.

**Household tagging (members).** Your other computers can tag this library with you. Pair a computer once under Settings → Household tagging; only reduced previews travel, only to computers you paired, over an encrypted connection on your home network. Originals, tags and the library stay on the library computer. Household tagging is part of the optional PhotoFindry membership: it needs an active membership key on every computer taking part, and it is the only members-only feature. Everything else stays free, and tags made with household tagging stay yours to search and export after a membership ends. Helping was tested between Apple-silicon Macs on build 142; the Linux build includes the same household code, but helping between a Mac and a Linux computer was not tested.

**Parallel tagging where it is faster.** Check performance now also measures how many photos this computer can tag at a time. New tagging jobs use more than one only when it was clearly faster with every sample accepted; otherwise tagging stays one photo at a time.

**Faster related searches on large libraries.** The search index now keeps exact per-photo vector counts, so related searches and background search indexing no longer recount the whole vector table. Existing indexes are counted once in the background by the queued Build semantic search job; until it finishes, coverage is measured the old way.

**Lens builder.** Find photos like… offers only photos a lens build can measure, counts the ones without a saved preview and offers to prepare their previews. Searches in the lens builder are no longer cancelled by gallery refreshes while the main search box holds a query.

**Removed photos.** Finder metadata files no longer block emptying Removed.

**Linux.** Build 142 brings household tagging, parallel tagging, exact vector counts and the Removed fix to Linux, with refreshed third-party notices. Showcase lenses remain Mac-only.

Updates preserve the library, originals, model paths, saved lenses and saved work. All inference remains local. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
