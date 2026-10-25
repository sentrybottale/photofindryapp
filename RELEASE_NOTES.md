# PhotoFindry 0.3.0-beta.1 · build 131

PhotoFindry is now available for Apple-silicon Mac, Linux x86_64 and Windows x64, with publisher-authenticated companion installers, signed updates, login startup and a local browser workspace. Install the companion for your platform at https://photofindry.com/docs/install.html.

Mac: an invalid saved folder grant after a volume identity change now offers the ordinary folder-selection action. The browser opens correctly after an update or rollback without waiting on the updater's own lock.

Linux and Windows: browser Settings/About offers signed update, recovery, rollback and startup controls. Maintenance runs independently of the requesting browser. The first release retains explicit per-platform feature limits: People is included on Mac/Linux; Windows has no face provider. Backup/source transfer and dedicated CLEF/Intern reviewers remain Mac-only. Linux/Windows use local BGE retrieval and the configured Qwen reranker without the Mac fast ranking provider.

All inference runs on the installed computer. Original photographs, libraries and selected model paths stay outside the program bundle. The client is MIT open source; the Rust engines remain proprietary and official builds remain free. The private implementation repository/history is not published.

The Mac package uses ad-hoc macOS code sealing and separate publisher Ed25519 signatures. Independently trust the first-install instructions, immutable setup bytes and existing publisher pin. Windows needs the Microsoft Visual C++ v14 x64 runtime and a working Vulkan driver. Linux requires a systemd user session; optional image formats need platform codecs. See QUALIFICATION.md for actual hardware, model timings and limits. The beta16 one-time Mac-app migration channel is unchanged.
