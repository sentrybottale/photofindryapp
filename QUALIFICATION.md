# Beta qualification

**Release:** 0.2.0-beta.2 · **Application:** 0.2.0 · **Build:** 75 · **Architecture:** arm64

The application declares macOS 14.0 or later; qualification used macOS 27.0
(26A428), Apple silicon and 8 GiB memory. Other OS/hardware combinations are not
claimed tested. Bundled Core ML semantic ranking requires macOS 15 or later.

## Feedback and release checks

- Swift build and 14 focused feedback/updater checks passed. Native render review
  covered empty, bug, suggestion, screenshot and unavailable-mail states.
- A real Apple Mail draft contained the intended recipient, report fields and a
  generic PNG attachment. Two synthetic drafts were discarded without sending.
  Returning to the form after Mail omits its cancellation callback was verified.
  Inbox delivery has not been tested; the user's email provider handles delivery.
- An independent MIME parser verified exported email text and attachments.
  Selected screenshot bytes remained unchanged; shared copies excluded original
  EXIF/GPS/comments and filenames. Visible image content is not automatically redacted.
- The exact app has 416 classified payload entries in 179 components, valid
  nested ad-hoc signatures and no unresolved packaging findings. The DMG was
  verified read-only against the exact candidate. The signed inventory, full
  update ZIP and update feed passed pinned-public-key verification.

## Signed update and saved data

A real Sparkle trial installed the exact beta-2 candidate over isolated beta-1
build 74. All 21 fixture-library file entries stayed unchanged during installation.
Reopening the installed worker preserved all 17 authoritative source tables,
accepted photo details, People edits and review history. Fixture originals and
selected model paths stayed unchanged. The installed customer app and library
were not targets of this test.

This new trial used a loopback-only HTTP server with real Ed25519 signatures. It
made no production server or feed changes. An initial test harness under Documents
failed to start its installer helper; the successful harness used a disposable
temporary directory and a framework inside its app bundle, without changing
system security settings. This is an external updater harness. Production UI
save/drain safeguards have separate automated coverage.

Beta 1 previously qualified the signed HTTPS delivery path, compatible manual
rollback, actual insufficient-space handling, bad feed/archive signatures,
corrupt archive, incompatible version/data contract, unavailable feed and check/
download cancellation. Those checks were not all repeated for beta 2. Authorization
denial, wrong bundle identity and interruption during final replacement remain
unqualified. An automatic downgrade is not supported.

## Reused runtime evidence and limits

All 20 native binaries other than the main app, bundled models and executable
entitlements match the qualified beta-1 runtime. Only the main UI executable,
build number, privacy manifest and source-build metadata changed. No inference,
store, schema, model-setting or updater installation code changed.

Prior bounded real-model tagging/refinement and positive face-fixture checks
are reused on that unchanged runtime; they do not establish broad model quality.
Bounded process observations saw loopback TCP only and may miss brief connections.
There is no OS-enforced network/filesystem sandbox or absolute offline guarantee.
Downloads, updates, browser links and voluntary feedback delivery use the internet.

The beta is not Apple Developer ID signed or notarized. Beta 1's browser-quarantine
trial recorded expected Gatekeeper refusal without executing the app or removing
quarantine. Successful first launch under a fresh macOS user remains unqualified.
The owner reports a signing-key backup; independent recovery has not been tested.

Component review is an AI-assisted packaging review, not an independent security
audit. The owner manages Codex Security for the private application-source
repository; its review of the beta-2 release commit completed on 29 September
2026. This does not certify security or future PR coverage. GitHub Actions jobs
could not start because of the account billing/spending limit; the build and
checks described above ran locally. This public repository contains distribution files; GitHub's automatic source archives contain those
files, not the application implementation.
