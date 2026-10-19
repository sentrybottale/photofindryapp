# PhotoFindry 0.2.0-beta.17 — CLI setup and retained-library recovery

This is a standalone CLI hotfix. Fresh installations use the short verified setup
command at app.photofindry.com. No PhotoFindry application is installed in Applications.

Setup now recovers the saved library location when an older app or companion
program has been deleted. Missing program files no longer hide an intact custom
library. A blocked or failed installation keeps that selection for Retry.
Existing CLI upgrades also repair idle same-library links before draining the
installed service. Active conflicting services and other libraries are preserved.
Executable launch checks, local ownership checks and signed update verification
remain enforced. Saved libraries, original photographs and model paths are kept.

The short setup command verifies a reviewable installer script before execution.
The script verifies the official CLI payload, installs login startup and opens the
local browser workspace. Existing companion users can choose Install signed update
in Settings → Background companion. If setup failed with “A registered local launch
path is unavailable”, use the current short setup command once more.

The older native app retains its beta 16 one-time migration channel. That bridge
exists only for existing app users; normal operation and fresh setup use the CLI.
The MIT client is open source; Rust engine cores and private history remain private.
No engine model/schema change, original-file modification or automatic retagging
is included. Community beta: separate publisher signatures, not Apple notarization.
