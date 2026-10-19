# PhotoFindry 0.2.0-beta.20 — library startup and recovery

This standalone CLI update addresses a startup failure reported after beta 19. The companion now names the stage of a slow library opening in local status and gives a specific error if it cannot finish. **Recover and open** tries the retained publisher-verified previous program when a newly updated program fails to open. It keeps the current library, originals, models and saved jobs; recovery never restores an older library backup.

The startup path uses a bounded read-only search-index check, and the AI search lane opens when needed. The ordinary search lane retains the ordering required by source recovery. No retagging or source-schema migration is part of this hotfix.

Fresh installation remains CLI-only. Beta 16 remains the one-time Mac-app migration bridge for existing native users. The affected other Mac has not yet been tested with this build; local qualification and limits are in QUALIFICATION.md.
