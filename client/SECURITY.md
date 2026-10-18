# Security and trust boundaries

The threat model includes a compromised public website, hosting VPS and malicious
web pages. The public origin must never receive a local pairing/session secret,
library response, arbitrary native action or engine-update authority. The installed
launcher accepts a fixed open URI or bounded archive navigation, never an endpoint,
command or file path supplied by a website. Local sessions are established by the
installed application; the companion enforces loopback Host/Origin checks,
authentication, finite actions and native consent for sensitive operations.

The generic Rust bridge is part of the open client. Its file checks, migration,
pairing, revocation, process ownership, feedback consent and HTTP controls can be
inspected here. PhotoWorker/PhotoEngine and native inference providers are separate
binary dependencies. A child process is not an operating-system sandbox. Client
source review cannot establish the absence of vulnerabilities in a private core.

Official macOS bundles use Contents/MacOS/PhotoFindry for the launcher and
Contents/Resources/companion/{bin,runtime,licenses} for client helpers and runtime.
The native updater verifies signed full-app updates against the installed pin,
drains owned work and replaces only the app. Migration preserves the existing
library location and verifies a private cold recovery copy before admission.
Uninstall preserves libraries, originals, models and backups.

Authenticate first-install artifacts and the exact source archive against a key
obtained independently of the download server. Checksums downloaded alongside a
file are not sufficient. A compromised download server can deny service or serve
old content; it cannot legitimately sign new bytes. Key compromise and malicious
local processes with the user's privileges remain distinct threats. Never accept
a new signing key merely because the website supplies it.

Send vulnerability reports through the repository's private vulnerability reporting
channel when enabled. Do not post exploit details or private data in public issues.
Until that channel is enabled, hold sensitive details and contact the maintainer
for a private reporting route. A prepared CI workflow is not proof that required
branch checks or every-push security reviews are enabled on a live repository.

The release gates and their exact evidence scope are described in TRANSPARENCY.md.
No claim of Apple notarization, OS-enforced engine isolation or flawless security
follows from this source publication.
