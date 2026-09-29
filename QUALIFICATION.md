# Candidate qualification

This source-free repository contains distribution documentation only. No first-party
application code, private development history, production key or release artifact
is included. This repository's publication is authorized; official app-binary
publication remains pending.

The first release must supply an exact version/build, artifact/component inventory,
third-party notices, signature verification key, authenticated checksums, official
HTTPS update feed and honest supported-platform/known-limitations record.

| Release gate | Status in this preparation snapshot |
| --- | --- |
| Frozen app version and monotonic build | Pending for the official release; local qualification used 0.1.54 build 73 |
| Owner-controlled signing identity | Created in Keychain; signed payload/feed verification and tamper rejection passed |
| HTTPS feed | Live signed empty feed; HTTPS 200 and exact bytes verified; no release items |
| Real signed update and compatible rollback | Not performed |
| Downloaded/quarantined clean-machine installation | Not performed |
| Local installation over an existing library | Passed for the development candidate; closed-library snapshot and authoritative source records preserved; real manual update check accepted the empty feed |
| Real-model and supported hardware qualification | Bundled tagging/refinement and positive face-helper smokes passed on the current Mac; broader hardware qualification pending |
| Runtime network observation | Bounded TCP sampling of owned model-smoke processes observed loopback only; this is not OS-enforced network isolation |
| Automatic PR-open/every-push security review | Owner reports Codex Security connected to this repository; completed PR-open/every-push coverage remains to be verified |
| Exact final payload disclosure/licence review | Local payload review complete; the final frozen release requires its own exact payload review |
| Explicit official binary publication instruction | Pending; public release-repository setup is authorized |

Local model qualification used one pinned upstream fruit photo. Tagging saved an
accepted result; refinement completed one round and two inspections with an
unresolved outcome while preserving the accepted record. A separate pinned face
sample produced one face and a normalized embedding. These checks do not establish
general model accuracy. Broader hardware and clean-install/update testing remain
required.

Network observation comprised 172 bounded samples, taken after state polls with
two-second pauses. Only loopback TCP sockets were observed. Short outbound attempts
can be missed; the observation does not prove offline operation or network isolation.

Do not publish unfinished candidates or claim the remaining gates passed from unit
tests. An asset-only repository security review does not examine the private
application or engine implementation. Their review and findings remain in
authorized private workflows. Signing credentials must never enter PR jobs or this
repository.
