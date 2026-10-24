# PhotoFindry

Local photo tagging and search for Apple-silicon Mac, Linux x86_64 and Windows x64. Your originals, models and library stay on your computer. Official builds are free; no subscription or PhotoFindry account is required.

[Install PhotoFindry](https://photofindry.com/docs/install.html) · [Open on Mac / platform setup](https://app.photofindry.com/) · [0.3.0-beta.1 release](https://github.com/sentrybottale/photofindryapp/releases/tag/v0.3.0-beta.1)

Fresh users install the signed CLI companion directly. It starts at login and opens a local browser workspace. Mac uses the retained native Open link; Linux uses the applications menu or `photofindry open`; Windows uses the Start menu. The beta 16 Mac application remains only the one-time migration route for older app users.

Use the pinned setup command for your platform. Independently trust its immutable script and verifier hashes and the existing publisher key; checksums obtained from a compromised website do not authenticate themselves. The signed per-platform feed authenticates updates. See [verification](VERIFY.md) and the release's signed `QUALIFICATION.md` for exact checks and limits.

Mac requires Apple silicon and macOS 14+ (Core ML ranking 15+). Linux requires x86_64 and a systemd user session. Windows requires x64, the Microsoft Visual C++ v14 x64 runtime and a working Vulkan driver. Vision model and projector files are selected separately. People is available on Mac/Linux; Windows has no face provider. Backup/source transfer and dedicated CLEF/Intern reviewers remain Mac-only. Mac packages use ad-hoc code sealing and publisher Ed25519 signatures, without Apple Developer ID/notarization.

The clean [client snapshot](client/) is MIT, covering the browser UI, native launcher, companion bridge and allowlisted interaction helpers. Rust engine cores and worker implementations remain proprietary. The private implementation repository and history are not published. See [licensing](LICENSE), [privacy](PRIVACY.md) and [source inventory](client/CLIENT-SOURCE.json).

Updates replace the program only. They preserve libraries, original photos, saved work and selected model paths. Keep independent backups; do not delete your library to repair startup. [Private feedback](https://photofindry.com/bugs) · [Optional support](https://buymeacoffee.com/photorecall).
