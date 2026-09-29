# Installing a qualified beta

No release is available yet. Once a qualified beta is
published, verify its final artifact signature using the published trusted key.
A checksum alone does not authenticate a publisher. Do not install a draft or
unsigned candidate merely because its filename resembles an official release.

Open the downloaded DMG, drag PhotoFindry to Applications, eject the image and
open the app. Vision models are separate: select a supported vision GGUF and its
matching projector from any chosen folder. Keep original-photo backups separately
from the app's library backup and cached previews.

The planned beta uses direct GitHub distribution to establish and test the project's
own release and update process before pursuing the separate Apple distribution
tracks. It is distributed outside the Mac App Store.
It uses ad-hoc bundle signatures and independently signed release/update artifacts.
It is not Apple-notarized or Developer ID verified. These distribution choices do
not establish a security guarantee. App Store qualification and notarization are
separate tracks; neither is being claimed by this beta.

macOS may block first launch. If you have verified and trust the specific release,
use Apple's per-app [Open Anyway instructions](https://support.apple.com/en-us/102445).
Do not disable Gatekeeper globally or recursively remove quarantine.

Updates will be explicit through Check for Updates. They replace only the app and
bundled runtime. Library databases, originals, retained previews, user edits and
selected model folders are not installer targets. Real signed update and compatible
rollback qualification are required before the first beta is released.
