# CLI companion lifecycle

Owner direction, 18 October 2026: the Mac application is a one-time migration
installer. Normal operation uses the installed CLI, a per-user LaunchAgent and
the local browser workspace. Submit as a focused hotfix, without repeating the
unrelated whole-release qualification suite. The existing publisher key is retained.

## Installation and daily use

New users install the CLI package directly. From build 120 the website provides
a stable Terminal setup pinned to an immutable content-addressed script and
native verifier. The retained publisher key authenticates the fixed feed before
setup parses its metadata or downloads the exact signed archive. A copied command
therefore continues to install later signed releases without changing its shell
bytes. It checks bytes before extraction/execution, then invokes the signed lifecycle
installer with this account's current-library selection and a fresh recovery path.
No application is put in Applications. The shell exits after setup; launchd owns
the service. First-install instructions/pins need an independently trusted source;
hashes supplied with malicious commands cannot authenticate those commands.

The application channel below exists only for existing native-app users.
The existing application update channel delivers one final migration installer
with the same bundle identifier. It verifies the publisher-signed CLI package,
stages an independent copy under `~/Library/Application Support/PhotoFindry Companion`,
and uses the established cold-copy/admission transaction for the library. The
library remains under `~/Library/Application Support/PhotoRecall`; originals and
selected model paths stay where they are. Existing jobs reopen paused.

After the service answers its private readiness check, the installer opens the
browser and exits. The service can move the verified, no-longer-running installer
to Trash; unexpected contents or changed identity prevent removal. The retained
native beta remains an explicit migration-failure alternative, not a permanent
host dependency. Dialog and fixed-link helpers are short-lived native processes.

`com.photofindry.companion` is a per-user LaunchAgent. Login startup does not open
a browser or resume the saved processing queue. **Open PhotoFindry** on the public
hub opens/restarts the registered local workspace. Closing a tab does not stop
jobs; Quit stops the service. Settings → Background companion exposes login
startup, automatic checks, signed installation and software recovery.

## Update authority and recovery

The feed is fixed at `https://photofindry.com/updates/cli/release.json`. Its bounded
JSON envelope contains a base64 payload and Ed25519 signature. The native verifier
pins the same publisher key already trusted by beta 13/14; it accepts no key from
the server and never reads the Keychain. Publisher signing remains a separate
local release action. The signature covers release/build/platform/data contract,
archive URL/size/hash and the complete package inventory hash. Only the fixed
GitHub distribution repository and exact versioned archive name are accepted.

The CLI authenticates archive bytes before libarchive extraction, rejects links,
traversal, duplicate entries and oversized content, then checks the complete tree
against the signed inventory. Versions are staged in a private directory and
published atomically. No downloaded shell script is executed. Current and previous
software are retained, with a highest-seen build watermark to reject feed rollback.

A durable transaction precedes registration/service changes. Update drains only
owned work, switches registration and the LaunchAgent, then requires local health
before committing. Interrupted or failed switches recover the prior verified
software. **Recovery never automatically restores an older library backup.**
User edits and completed work stay in the current library. Explicit software
rollback does not lower the anti-replay watermark.

A separate, transient maintenance LaunchAgent performs browser-requested and
explicit-Open updates
and uninstall so stopping the main service cannot kill its updater. Browser
operations are authenticated finite action names; they carry no executable URL,
trust key or filesystem authority. The public website has no update/install API
access. Background checks report availability; they do not silently install.
An explicit **Open PhotoFindry** checks the authenticated feed and installs a
newer signed build before opening when owned work can safely drain. Offline
checks and foreground work keep the current verified build available and show
a local warning. The background-check switch does not disable the Open check.

Uninstall requires the existing local confirmation, drains owned work, disables
login startup, removes its LaunchAgent and moves the verified active program
package to Trash. The library, originals, models, backups and inactive recovery
versions remain. No background registration can resurrect the removed program.

## Focused verification

- Companion unit regressions include the private health handshake, admission,
  signed metadata spelling, archive traversal/link/duplicate rejection and leases.
- Migration controller/process tests cover CLI handoff, cancellation, failure
  recovery, fixed public links and keeping the background service alive when the
  installer closes. Browser handler tests exercise finite lifecycle controls.
- `scripts/test-cli-lifecycle.py` uses a marked disposable installation, an
  explicitly selected closed fixture library and signed candidate packages. It
  exercises real launchd login startup, authenticated controls, wrong signatures,
  killing an updater after its durable switch, software rollback, saved-evidence
  preservation and confirmed uninstall. It never targets the owner's library.
- `scripts/qualify-cli-core.py` runs against an already installed, signed
  candidate in an isolated, explicitly marked library. It uses the installed
  browser API and real local models to check ordinary search, a completed
  refined AI search, explicit single-photo tagging, Cancel all, Quit/Recover,
  then confirmed uninstall. Its receipt records timings and verifies the
  original photo bytes, catalog header, photo count and pre-existing jobs.
  Run this after installation on a representative full-library copy for each
  release; a responsive-layout test or empty-library lifecycle test cannot
  substitute for it. Keep any failed fixture for diagnosis and do not publish
  until its installed workflows pass.
- The opt-in Rust `signed_candidate_archive_and_tampering` test verifies the exact
  signed envelope, archive extraction and tamper rejection with the shipped helper.

Candidate results and publication receipts belong in the hotfix record. A passing
small fixture is not a claim of another-Mac acceptance, notarization, a full engine
requalification or an independent security review. Required public repository
checks and the client/proprietary-core export boundary remain in force.

## Post-beta-20 Open and installer repair — 20 October 2026 candidate

The installed beta-20 script copied signed build 118 on the owner's Mac, but
its older registered development broker only understood Open. Setup could not
receive `prepare-update` and remained unregistered. After a controlled Quit,
the same signed build opened the library but LaunchServices resolved its Open
link to an older bundle with the same internal helper identifier; setup restored
the old registration. The next installer uses a one-use same-user broker ticket
and authenticated local API to check the registered library and idle state
before stopping an Open-only host. It refuses active legacy work.

Each new versioned Open-link helper has its own internal build identity so
retained launchers do not compete for one LaunchServices identity. The main
PhotoFindry application identifier, library directory and saved data keys do
not change. A visible native progress panel covers the signed check, package
verification and opening wait. A retained exact signed version may be reused
after full package verification instead of downloaded again; the highest-build
watermark and startup health check still apply. If Open finds foreground work,
it defers the update without downloading the archive. Background work may be
drained by the authoritative updater, while an offline check opens the current
verified build and reports the failure locally. Direct `lifecycle open-update`
uses the same signed path for CLI recovery and isolated qualification.

## Account-installed entry and failure isolation — build 120 candidate

Build-scoped identities did not prevent multiple test copies of the same helper
from claiming macOS links. From build 120, packaged helpers declare no product
URL schemes. Only the generated `entry/PhotoFindry.app` under the account's
installation root declares them, with a unique stable installation identity.
This short-lived helper verifies publisher-signed inventory and exact controller
bytes before running them. It also retains an independently authenticated small
recovery controller, so missing current software does not need to execute itself.

A read-only probe in launchd's actual background context checks folder access
before stopping the old companion. Software readiness and link selection are
separate outcomes: a failed link repair keeps healthy software running and records
its specific failure. The durable last failure survives rollback and an absent
installation record. Explicit Open maintenance has a nonce-bound completion
receipt and survives the requesting launcher closing. Unexpected crashes of a
previously healthy service receive bounded restart attempts; initial failure,
Quit and maintenance shutdown do not become restart loops.

Transient browser failures retry reads with bounded backoff, preserve visible
results and pending edits, and never replay mutations. A persistent panel offers
native Open and verified installation repair. Automatic cross-process tab handoff
is not implemented; native Open obtains a fresh authenticated origin/session.
See the private architecture record for incident evidence and the remaining
authenticated control-plane design.

## Launch routing correction — 19 October 2026 (unreleased)

Beta 15 still exposes `photofindry-local://open` on the public hub. Retained
browser-app copies advertise that same scheme, and LaunchServices can reopen one
after the application in Applications is removed. The same-build installer path
also omitted native link repair. Reusing a build number for different signed
program bytes could silently keep the old installed program; the next installer
rejects that case with an explicit instruction to use a newer verified build. The beta 15 custom-root lifecycle qualification
intentionally skipped production URL registration; parser tests did not qualify
this actual platform association.

The next client uses `photofindry-companion://open` for public entry and local
recovery. Only the short-lived companion helper advertises it; the one-time
installer advertises no URL schemes. Legacy local/archive links remain accepted
by the helper. Registration always selects the helper, including when invoked
inside an installer; never the enclosing application. A real desktop test uses
unique protocol aliases in Application Support, registers a previous app,
deletes the old installer, lets the previous app reclaim its legacy link, then
opens the companion link through LaunchServices and observes only the helper's
fixed `open-companion` command. It does not alter the owner's product handlers.

Reinstalling the same build repairs native links. Default opening prefers the
verified standalone installation over a stale same-library registration. Repair
preserves active legacy work and tells the user to quit that copy before retry;
it may replace an idle same-library binding, never another library. Recovery also
repairs missing registration without restoring a backup. Normal current launches
need no service restart or full package rehash.

Roll out the companion first, using a new official build (114 or newer), then
switch the public hub. The prepared website branch gates deployment on that
minimum verified release and a matching download link. The owner authorized the coordinated build 114 hotfix on 19 October.
Publication still depends on exact-candidate upgrade and delivery qualification;
this is not an other-Mac acceptance claim.
