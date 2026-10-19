# PhotoFindry 0.2.0-beta.19 — companion and search recovery

This update repairs background companion reopening after Quit and makes signed
update failures identify their cause and recovery state. It keeps the library,
original photographs, selected model files, saved jobs and reviewed matches.

Large-library search returns every match while bounding optional local related
reordering. A queued AI search shows retrieval progress and opens its unscored
All candidates pool until Best so far contains scored results. Stop all remaining
retries a lost local acknowledgement for the same library and shows the committed
queue state. No retagging or library schema migration is part of this hotfix.

Fresh installations remain CLI-only. The beta 16 Mac application remains the
one-time migration bridge for existing native-app users. The public website has
no library authority; the signed CLI feed remains the update trust boundary.
