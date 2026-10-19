# PhotoFindry 0.2.0-beta.18 — tagging and automatic source recovery

This CLI hotfix fixes failed tagging during bursts of streamed progress. Progress
now waits within bounded transport queues; cancellation and real frame limits
remain enforced. A single OCR format repair uses a narrower checked decoder when
copied text exceeds the aggregate schema bound; final validation remains strict.

New tagging, metadata, preview, face and improvement jobs retain unavailable
selections and share durable source recovery. Cached previews remain usable offline.
The native filesystem observer also reconciles returns during startup.

Verified originals that return to the same saved location automatically release
their existing jobs. Missing originals keep saved tags and previews while other
usable photos continue. Changed or uncertain locations still require identity
confirmation. Folder recovery follows its exact source/root and keeps the waiting
job linked through acknowledged relocation. Pauses, stops, schedules and resource
holds remain in force. Quick location checks do not start a camera/location scan.

Explicitly moving entries to Removed skips those pending targets once across
source-reading jobs, including ready, offline and identity-held targets. Other
records, original photographs and saved accepted details stay intact. Recovery
selection uses an index, including a 95,000-target regression fixture.

Install the CLI directly using the verified setup at app.photofindry.com. Existing
companion users choose Install signed update in Settings → Background companion.
The beta 16 application channel remains only the one-time migration bridge for
existing native users. Libraries and selected model paths stay in place.

The reviewed MIT client is open source; Rust engine implementations and private
history remain private. Community beta artifacts use the retained publisher key;
they are not Apple Developer ID signed or notarized. See QUALIFICATION.md for the
exact scope and limits of testing.
