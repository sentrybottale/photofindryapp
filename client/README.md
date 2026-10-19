# PhotoFindry client

The Mac client is MIT licensed. It contains the Svelte browser UI, native launcher,
local authenticated bridge and macOS interaction helpers. PhotoWorker, PhotoEngine,
their contracts and inference policy implementations remain proprietary and are
distributed separately in official companion packages. This is not a source build
of those engines. Official companion builds remain free to use.

The browser workspace is served by the installed companion on loopback. The public
website can request that the installed launcher open it, but does not receive
library authority. Photos, models and inference stay local. Optional, explicit
feedback and model downloads have their own visible network actions.

## Installed lifecycle

New users install the CLI directly using the verified setup at
https://app.photofindry.com/#installation. No PhotoFindry application is installed
in Applications. Only existing native-app users use the application update channel
for its one-time migration bridge: it verifies a recovery copy when needed, installs
the independent CLI and exits after the local service is ready. The per-user LaunchAgent starts the companion at login; native file dialogs
and the URL handler run only when needed. No permanent Mac application is required.

Use **Background companion** in browser Settings for login startup, automatic
update checks, signed installation and software recovery. The installed publisher
key verifies the fixed CLI feed and exact package bytes. Updates keep a previous
software version; recovery never automatically restores an older photo library.
Automatic checks do not silently install. Uninstall disables startup and keeps
saved libraries, original photographs and model paths.

## Build and test the client

Use macOS 14 or newer on Apple silicon, Swift 5.9 or newer, Node 24 or newer,
Python 3.11 or newer, and the exact Rust toolchain in `engine/rust-toolchain.toml`.
The lockfiles pin dependencies; dependency acquisition needs network access or an
existing cache. Nothing in these commands downloads a proprietary engine.

```sh
cd web-companion/frontend
npm ci --ignore-scripts --no-audit --no-fund
npm test
npm run check
cd ../..
python3 scripts/verify-web-frontend.py
node --test tests/*.test.js
cargo test --locked --manifest-path engine/Cargo.toml
cargo build --locked --release --manifest-path engine/Cargo.toml
swift test
swift build -c release --product PhotoFindryBrowserHost
bash scripts/build-companion-tools.sh
```

The companion embeds the retained frontend assets at build time. After changing
Svelte code, run `npm run build` and `npm run check` in its frontend directory.
The backend requires separately authenticated PhotoFindry runtime binaries.
Do not treat a source build as an official signed release or point development
builds at your only library. Use an isolated new library and verified runtime for
integration work. The launcher expects the official bundle layout described in
`SECURITY.md`; `swift run` alone is not an installer.

The source snapshot has no private repository history, user data, credentials,
engine source, inference recipes or model weights. `CLIENT-SOURCE.json` binds its
exact file bytes. Its hashes detect changes; authenticate the signed release
inventory with an independently trusted publisher key before trusting a download.

This source snapshot is separate from the official signed companion download.
Consult the distribution repository’s release page for published builds. Source
availability does not grant redistribution rights to the proprietary engines.

## Contributions

Keep local authentication, resource bounds, original-photo preservation and clear
recovery paths intact. Run client checks on every change. Never attach personal
photos, libraries, model weights, access tokens or unredacted diagnostics to issues.
See `SECURITY.md`, `TRANSPARENCY.md` and `THIRD_PARTY_NOTICES.md`.

For a rehearsal handoff, use `python3 scripts/verify-handoff.py --handoff
/absolute/handoff --allowed-signers /independently/trusted/allowed-signers`.
The verifier never installs or executes the download, and always labels rehearsal
keys and evidence as insufficient for a production release.
