# Beta qualification

**Release:** 0.2.0-beta.1 · **Application:** 0.2.0 · **Build:** 74 · **Architecture:** arm64

The application declares macOS 14.0 or later. Qualification on this Mac used
macOS 27.0 (26A428), Apple silicon and 8 GiB memory. Declaring an older minimum
is not proof of testing every supported OS or hardware configuration. Bundled
Core ML semantic ranking requires macOS 15 or later and is unavailable on macOS 14.

| Check | Result |
| --- | --- |
| Exact release build and component/disclosure review | Passed: 416 payload entries classified in 179 components, with no unresolved packaging findings |
| Ad-hoc bundle integrity and nested signatures | Passed for all 21 native binaries; read-only mounted DMG contains the exact reviewed app |
| Maintainer-signed DMG/archive inventory and signed feed | Passed independent public-key verification; modified inventory, artifact and signed payload checks were rejected |
| Signing-key recovery | Owner confirms a backup; independent restore verification has not been performed |
| Real signed application update | Real Sparkle installation from isolated build 74 to compatible test build 75 passed over HTTPS |
| Compatible rollback and fixture data | Manual app-only rollback to 74 passed; 17 authoritative source tables and fixture originals/model paths remained unchanged |
| Insufficient installation space | Actual target-volume exhaustion was handled without changing the existing app; the old worker reopened the unchanged fixture successfully |
| Browser download and quarantine | Exact DMG downloaded in a browser; quarantine retained through read-only mounting and private installation copy; Gatekeeper refused execution as expected |
| Successful fresh-user first launch | Not performed; the quarantine trial intentionally stopped before executing app code |
| Main vision tagging/refinement and positive face fixture | Prior bounded real-model smokes passed; all 20 native binaries other than the main app, model bytes and executable entitlements match the exact final release |
| Existing-library preservation | Development candidate replacement preserved a closed-library snapshot and all authoritative source records |
| Public repository security integration | Owner reports Codex Security connected to this release repository; completed PR-open/every-push reviews have not been independently verified |
| Application-source security review | Owner manages the separate private-repository integration; no completed scan or PR coverage has been independently verified |

## Update and rollback scope

The update trial used the real Sparkle framework, signed HTTPS feed and signed
application archive. Its private fixture applications used the release's compiled
code with isolated preference metadata; test build 75 changed only its build
number and that isolation metadata. No test build is offered as an official release.

Before the first worker launch after installation and rollback, all 34 fixture
filesystem entries matched their baseline. Running the bundled worker after each
replacement preserved 17 authoritative tables and retained a real accepted result,
preview, user-authored People name and undo history, and review history. Fixture
originals and selected external model paths stayed unchanged. The existing installed
app and its updater preferences also remained unchanged.

Rollback was an explicit, compatible manual app replacement. The updater rejects
older builds; this was not an automatic downgrade. The trial covered rejection of
a tampered feed, older build, future macOS requirement, incompatible data contract,
a signed corrupt ZIP, an incorrect archive signature and unavailable feed, plus
cancellation of a check and a partial download. The incorrect signature was
rejected after the full download and before extraction; the existing fixture app
was unchanged. A bounded target volume then ran out of space during the actual
new-app copy: installation stopped, the existing app retained all 667 compared
entries and valid signatures, and its worker reopened the same fixture with all
17 source tables unchanged. The reported outer Sparkle error was generic; detailed
diagnostics recorded insufficient space. The external harness is not evidence
that the production application's complete updater UI, save fencing, active-job deferral or shutdown
path was exercised. Those paths have separate automated coverage.

Authorization denial, an incorrect bundle identity and interruption during final
replacement have not been tested in this trial. Broader hardware, large-library,
quality and recovery testing remain ongoing beta work.

## Model and privacy scope

Prior model qualification used one public fruit image: tagging saved an accepted
result and one refinement round completed with an unresolved outcome while retaining
the accepted result. A separate public face sample produced one face and a normalized
embedding. These checks do not establish broad model accuracy.

Bounded observation of those owned processes saw only loopback TCP connections.
Short connections may be missed. This is not proof of an OS-enforced network sandbox
or an absolute offline guarantee. Downloads, updates and user-opened browser links
require internet.

The beta is not Apple Developer ID signed or notarized. Browser-downloaded bytes
matched the authenticated release inventory; macOS quarantine remained present.
The expected Gatekeeper refusal was recorded without running the app, removing
quarantine or changing global security settings. Successful launch after a user's
per-app exception is a separate check and has not been qualified on a fresh user account.

The disclosure/component review is an AI-assisted packaging review, not an
independent security audit. Security scanning of this release-only repository
cannot inspect private application source. GitHub's automatic source ZIP/tar.gz
links snapshot this repository's public files, not the application implementation.
They are not a substitute for connecting the private source repository to the
review integration.
