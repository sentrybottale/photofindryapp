# PhotoFindry 0.3.0-beta.4 · build 137

This hotfix updates the Mac companion. Linux keeps 0.3.0-beta.3 / build 136 and Windows keeps 0.3.0-beta.2 / build 132.

Build 136 re-read every original photograph after a cloud drive such as pCloud remounted, or after the same external drive was plugged in again. On a cloud drive this downloaded the whole folder one photo at a time, and the source check restarted from zero. Returning folders now reconnect by comparing each file's size and dates, as in earlier releases. A file replaced in the same place still gets a new change time and is compared against its saved checksum before its tags are used. The byte-only mode that build 136 saved for these folders is removed automatically; no folder selection, review or retagging is needed.

Updates preserve the library, originals, model paths and saved work. All inference remains local. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
