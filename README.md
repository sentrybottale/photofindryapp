# PhotoFindry

**Find the photo you remember. Keep your library on your Mac.**

PhotoFindry is a native macOS app for local photo tagging, people and search.
Browse your folders, find photographs from saved details, and choose local AI
search when you need a closer look. Your originals stay in place.

[Download the first beta](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.2.0-beta.1) · [Installation](INSTALLATION.md) · [Website](https://photofindry.com)

## First beta: 0.2.0-beta.1

For **Apple-silicon Macs**. The application declares **macOS 14 or later**; current
qualification was performed on macOS 27.0. Bundled Core ML semantic ranking
requires macOS 15 or later. See [qualification and limitations](QUALIFICATION.md)
for the tested scope. This is early software: begin with a small photo folder and
keep independent backups of your originals and saved library work.

- **Local processing.** Tagging, faces and search inference run on your Mac.
- **Your models, your folders.** Select a supported vision GGUF and matching projector
  from any folder. Search and face models are bundled; optional downloads are explicit.
- **Saved work stays yours.** Browse, read and export your tags, names and evidence.
- **Free to use.** Personal and business use, with no mandatory PhotoFindry account,
  subscription or feature gates.
- **Updates when you choose.** Use **Check for Updates…**. Signed updates replace
  the app bundle while preserving the library, originals and chosen model paths.

Internet is needed for explicit downloads, update checks and opened web links.
The app has no advertising analytics or app telemetry. Read the complete
[privacy explanation](PRIVACY.md); local processing is not a claim of an OS network sandbox.

## Direct distribution

PhotoFindry is distributed through GitHub Releases. This beta uses ad-hoc macOS
bundle signatures and independently signed download/update artifacts. It is
**not Apple Developer ID signed or notarized**, and is outside the Mac App Store.
This keeps the first beta on the selected direct-distribution path; it does not
replace Apple's review or establish a security guarantee. Follow the per-app
installation instructions instead of disabling macOS protections globally.

PhotoFindry and its engine are proprietary and free to use under the [licence](LICENSE).
This public repository holds official binary releases and documentation. Application
source and development history remain private. Third-party licences and notices
are included in the app.

GitHub's automatic **Source code (zip)** and **Source code (tar.gz)** links contain
this release repository's documentation and verification files, not the application
source. Install the named PhotoFindry DMG asset. App-source security reviews require
access to the separate private development repository; scanning this public
repository does not establish that the app has passed a security scan.

## Help shape the beta

[Report a problem](https://github.com/sentrybottale/photofindryapp/issues) with your
app version, macOS version, Mac model and short reproduction steps. Avoid posting
private photographs, libraries, model weights, names, queries or unreviewed logs.

If PhotoFindry helps you find something worth remembering, you can
[buy us a coffee](https://buymeacoffee.com/photorecall). ☕ Completely optional.

[Verify a download](VERIFY.md) · [Qualification](QUALIFICATION.md) · [Privacy](PRIVACY.md)
