# MIT browser client

PhotoFindry uses Svelte 5 to compile the browser frame and its accessible responsive navigation. The production application remains a static, locally bundled client. It does not run SvelteKit, a development server, Node, a CDN or a cloud frontend service.

The shell mounts synchronously into `#photofindry-root` through `window.PhotoFindryShell.mount()` before the established feature controllers start. Scripts, styles and document resources remain outside that root. Every controller ID in `legacy-dom-contract.json` stays available. The gallery images/selection, inspector, filter editors, activity, onboarding and preview bodies are intentional content islands: their existing bounded controllers retain photo data, request cancellation, image ownership, job semantics and model metrics. Shell updates do not duplicate or recreate that data.

The Svelte components own grouped navigation, a bounded folder tree, current-page photo summary metadata, saved Light/Dark/System appearance, status/density controls, compact navigation and inspector state, focus trapping/restoration, Escape/backdrop recovery, native-dialog precedence and responsive lifecycle cleanup. Mobile session controls forward to the existing consent and disconnect handlers.

From this directory:

```sh
npm ci --ignore-scripts --no-audit --no-fund
npm run build
npm run check
npm test
```

All dependency versions and integrity records are pinned in `package-lock.json`. Once installed, building and checking need no network; a dependency cache can also supply `npm ci --offline --ignore-scripts --no-audit --no-fund`. Optional esbuild platform binaries come from the pinned packages; install scripts are disabled.

Generated `../ui-framework.js`, `../ui-framework.css`, `../ui-framework.NOTICES.txt` and `../ui-framework.manifest.json` are retained for offline application packaging. The manifest binds exact source and output hashes without a timestamp, so a repeat build produces identical files. `npm run check` fails for missing or stale generated assets. The JavaScript budget is 96 KiB uncompressed and 36 KiB gzipped; no dynamic imports, source maps, development tooling or runtime package loader are emitted. Bundled runtime notices accompany the compiled assets. Compiler, bundler and DOM-test dependencies are build-only and must never enter the installed runtime allowlist.

Tests run the actual compiled bundle in an isolated DOM with network requests rejected. They cover all existing controller IDs, actual reactive drawer behavior, keyboard/focus and modal recovery, retained imperative content, session-action handoff and lifecycle cleanup. Real Safari layout and bundled companion checks remain necessary before activation.
