# PhotoFindry

Local photo tagging, people and search for macOS.

**The first beta is being prepared. No app download is available yet.**

This repository is reserved for official PhotoFindry binary releases and their
verification, installation and privacy documentation. PhotoFindry is proprietary
and free to use for personal and business use. Application and engine source are
private. Third-party components retain their own licences and notices, included
with the application.

- [Website](https://photofindry.com)
- [Official releases](https://github.com/sentrybottale/photofindryapp/releases)
- [Report an issue](https://github.com/sentrybottale/photofindryapp/issues)
- [Optional support: Buy Me a Coffee](https://buymeacoffee.com/photorecall)

This repository contains release documentation and public verification metadata.
Please do not submit application source, private diagnostics, photo libraries or
original photographs in issues or pull requests.

The first beta is planned for Apple-silicon Macs. Actual minimum macOS and tested
hardware will be specified with the frozen release. Use only the official release
and verify the authenticated final artifact inventory before installing.

See [installation](INSTALLATION.md), [privacy](PRIVACY.md),
[qualification status](QUALIFICATION.md) and [terms](LICENSE).

The official beta feed is `https://photofindry.com/updates/beta/appcast.xml`.
`update-verification.json` contains its public verification key. The signed empty
`updates/beta/appcast.xml` is live at this URL, with HTTPS delivery verified
byte-for-byte. It contains no release items.
The private signing key is never part of this repository.
