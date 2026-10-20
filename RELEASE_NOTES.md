# PhotoFindry 0.2.0-beta.21 — reliable CLI opening and recovery

This standalone CLI hotfix repairs setup when an older companion understands only the original Open request. It checks that the previous service owns the same library and is idle, then shuts it down through its authenticated local API before installing the signed replacement. The internal macOS link handler now has a build-specific identity so retained development or older copies cannot claim the new Open link. The main PhotoFindry identifier, saved library and original photographs remain unchanged.

Every explicit **Open PhotoFindry** checks the fixed publisher-authenticated CLI feed. When a newer signed build is available and owned work can safely drain, the CLI installs it before opening the local workspace. Offline checks or active foreground jobs open the verified current version and explain the deferred update in the local browser. A second Open waits for an update already in progress. The native launcher shows progress during a slow check or switch.

One-photo scoped AI searches skip retrieval phrases that cannot add another candidate. The Jobs summary also distinguishes originals needing same-folder identity confirmation from unavailable originals. No routine retagging, original-file rewrite or library schema migration is part of this release.

If an older installation will not open, rerun the [verified CLI setup command](INSTALLATION.md#install-the-cli-companion-directly); it can repair the installation without opening the broken program. The beta 16 native-app migration channel is unchanged. The affected other Mac was not directly tested; see [qualification and limits](QUALIFICATION.md).
