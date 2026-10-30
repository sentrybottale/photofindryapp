# PhotoFindry 0.3.0-beta.3 · build 136

This release updates Mac and Linux. Windows keeps its existing signed 0.3.0-beta.2 / build 132 release.

Returning originals reconnect automatically when their identity and retained folder access can be verified. Files, Jobs and the activity monitor show the same status and a direct next action when access or ambiguous identities need your attention. Folder reviews show progress, survive reopening, and can be cancelled. A review no longer holds all processing. Explicitly paused or stopped work stays stopped. If the app closes while Stop is still draining, reopening preserves the cancellation and keeps completed work.

Ordinary search starts with Related matches switched off, matching the native app's default. It avoids repeated full-library preparation and reuses current matches for repeated searches and paging. The first search after an upgrade may take time to validate saved evidence; that validation is retained for unchanged evidence. Broad first searches can still take longer on large libraries. Saved descriptions, People, location evidence and match coverage are preserved; no retagging is required.

Linux browser-requested updates and recovery no longer deadlock while handing control from the maintenance parent to the installed program.

Updates preserve the library, originals, model paths and saved work. All inference remains local. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
