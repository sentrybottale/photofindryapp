# Client dependencies and assets

First-party files in CLIENT-SOURCE.json are MIT licensed, except explicitly
embedded upstream notices and third-party licence texts, which retain their terms.
The PhotoFindry name and logo identify the product; this copyright licence grants
no right to imply endorsement or use the maintainer's signing identity.

- Svelte, esm-env and clsx: MIT; complete runtime texts are retained in
  web-companion/ui-framework.NOTICES.txt.
- esbuild and happy-dom: build/test dependencies; versions and package integrity
  are pinned in web-companion/frontend/package-lock.json. Preserve their included
  licences when redistributing those packages.
- Sparkle 2.10.0: MIT and its included third-party notices; its binary archive is
  pinned by the Swift package checksum. Official packages include its LICENSE.
- Rust dependencies: engine/Cargo.lock pins the bridge's dependency closure.
  Official binary distributions retain the complete linked Rust notices and
  inventory. Source dependency metadata is not a substitute for those notices.
- Decorative placeholder images were generated for this project. Their provenance
  is in web-companion/assets/PROVENANCE.json; they do not depict user photographs.
- The glyph helper uses installed system emoji fonts; no Apple font or raster atlas
  is redistributed. The separately bundled Unicode catalogue retains its notice.

Proprietary engine binaries, third-party model weights and native inference
providers are not included in this client source archive. Their individual terms
and notices accompany the separately distributed runtime.
