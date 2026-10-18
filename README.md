# PhotoFindry

**Your photographs, rediscovered. Local processing on your Mac.**

The browser rewrite uses an independent background CLI companion with a
Svelte interface. Open [app.photofindry.com](https://app.photofindry.com) to launch
the installed workspace. The public site receives no library access; photos,
models, tagging, people and search stay on your Mac.

The [client source](client/) is MIT licensed. It includes the browser UI, native
launcher, generic Rust companion bridge and selected macOS interaction helpers.
The Rust engine cores remain proprietary, supplied in official builds. Rust is a
language, not the licensing boundary. Official builds remain free to use.

## Current beta: 0.2.0-beta.15

[Download the Mac browser beta](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.2.0-beta.15)
· [Install and migrate](INSTALLATION.md) · [Qualification](QUALIFICATION.md)

Build 113 uses a one-time installer, then runs without a permanent Mac app. Login
startup, signed CLI updates and recovery are controlled in the local browser.
Libraries, originals, model paths and saved work stay in place. Verify the signed release inventory before installation.

The client directory is the exact reviewed snapshot associated with the release
candidate. Its 136 files plus CLIENT-SOURCE.json match the signed source archive.
This root page records publication status. The private development history and engine sources
are not part of this repository. Source changes require fresh inventory review.

## Build and contribute

Start with [client build instructions](client/README.md); commands run inside
`client/`. Official engine binaries are separate dependencies. A source-only build
does not include them or become an authenticated official installation.

Required CI builds/tests the actual client and scans Actions, Python, Swift,
JavaScript and Rust. Main requires completed build, source-boundary and security checks. The owner
controls publication; there is no mandatory second-person approval. See [security reporting](SECURITY.md), [client trust boundaries](client/SECURITY.md),
[transparency](client/TRANSPARENCY.md) and [download verification](VERIFY.md).

Existing native-app users keep their library, original photographs and selected
model paths. Browser migration offers a verified recovery copy and a Use current
beta alternative. Imported sources may need their originals folder confirmed on
the current Mac. Never restore an older library just to switch interfaces.

Apple-silicon Mac, macOS 14 or newer; Core ML semantic ranking needs macOS 15.
Community betas use ad-hoc application signatures and independently signed
artifacts. They are not Apple Developer ID signed or notarized.

[Website](https://photofindry.com) · [Private feedback](https://photofindry.com/bugs) ·
[Optional support](https://buymeacoffee.com/photorecall)
