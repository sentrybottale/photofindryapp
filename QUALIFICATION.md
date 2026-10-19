# Beta 19 / build 117 qualification

The owner approved publication on 2026-10-19. This is the CLI-only successor to
beta 18. Beta 16 remains the existing-user native migration channel. The public
client is an exact MIT allowlist snapshot; private engine source and Git history
must not be exported.

The build-117 package was publisher-signed with the retained updater key
`G363x/YCT07qbJyyI5minQjp3v+6SEw9sB7dU3pL6cM=`. Candidate archive SHA-256 is
`f0e9d882279ea08058c655a2a7faa13a8b7ab512588f0fbfe700ee17e8c3ea36`
(397,878,185 bytes); signed CLI envelope SHA-256 is
`3ada94ea93e68e6440af4816f0b78fd8b303fd908acb69d22be78de3e0b4ec55`.
The archive and envelope must match the public release assets exactly.

Matched package qualification passed 1,513 Node checks with 23 explicit skips,
377 worker tests with ten opt-in skips, 39 engine tests, 137 companion tests with
two opt-in skips, 28 handover tests, Swift build, Svelte verification, and exact
138-file client allowlist hashes. The small isolated signed lifecycle suite passed
all 11 checks: real launchd autostart, authenticated controls, forged-signature
rejection, killed-update recovery, signed successor, rollback, anti-replay,
library/model preservation, and uninstall preservation.

On a separate 93,806-photo library clone, a forged successor was rejected
without stopping the service. Signed 116-to-117 installation took 45 seconds,
the first gallery page loaded in seven seconds, and immediate Quit/Open completed
in one request in 43 seconds. The service stayed healthy afterward. Explicit
software rollback and signed retry also succeeded. Photo, job and review row
counts and the catalog header hash matched the baseline; the queue stayed
paused. A packaged ordinary search returned all 4,921 matches in 16 seconds
idle or 26 seconds under competing worker tests; Stop all committed in 42 ms.
The queued AI search retained 22,190 candidates, completed 509 preview reviews,
populated Best so far with 60, and preserved those results after Stop/restart.

The direct signed 115-to-117 isolated lifecycle suite passed its ten applicable
checks, including killed-update recovery, signature rejection and rollback. The
affected other Mac has not been directly inspected or upgraded. The signed
full-size test starts from build 116. Full Qwen refinement of all 22,190 candidates, reboot
on another Mac, notarization and an exhaustive independent security audit are
not claimed. The 43-second Quit/Open wait is visible; this release prevents the
rapid-retry failure but does not yet make a full-library restart instantaneous.
