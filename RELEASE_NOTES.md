# PhotoFindry 0.2.0-beta.16 — companion launch and job recovery

Open PhotoFindry now targets the standalone companion through a dedicated link.
Retained older apps cannot claim this new entry; the one-time installer advertises
no workspace links. Reopening setup repairs the installed helper selection. The installer follows your already registered library, including a library outside the default folder.

- Missing or stale launch registration repairs safely for the same library. Active
  older work is preserved, with a clear quit-and-retry action.
- Failed improvement answers get one scoped automatic correction. Rejected
  answers never overwrite accepted tags; repeated failure explains Retry or
  changing the photo review model. Historical validation details remain available.
- AI search retains long saved evidence in bounded scoring passages instead of
  failing the whole search at a 24 KiB record. Every retrieved candidate remains,
  and interrupted passage scoring does not commit a partial photo score.
- Terminal AI failures show the actual cause and recovery action.

Existing companion users can install the signed update from Settings. Older or
incomplete installations should download the verified installer once, open it,
then return to app.photofindry.com. The installer exits; normal use runs through
the background CLI and short-lived native helpers. Photos, tags, People, model
paths, saved work and recovery copies remain in place.

The MIT client is open source; the Rust engine cores and implementation history
remain proprietary. No model weights, data schema or original files are changed.
Official builds remain free. Community beta: ad-hoc macOS signing with separate
publisher-authenticated artifacts, not Developer ID signed or notarized.
