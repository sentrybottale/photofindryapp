# PhotoFindry 0.3.0-beta.11 · build 149

This release brings Windows up to date and lets a Windows PC help another computer tag photos. Mac and Windows build 149 were published on 27 November 2026. Linux keeps 0.3.0-beta.8 / build 142 on its own signed feed.

**Household tagging on Windows (members).** A Windows PC can now lend its graphics card to a PhotoFindry library on another computer: under **Settings → Household tagging → Help another computer**, turn on **Let paired computers use this computer to tag their photos**, pair once with the six-digit code shown on both computers, and the library sends photos to the PC while it tags. Earlier Windows builds did not include these screens. In our test, a Mac library tagging 18 photos with a Windows PC helping finished in 98 seconds instead of 287 seconds on the Mac alone. Household tagging needs a membership key on every computer that takes part.

**Helpers behind a firewall.** A helper whose firewall refuses incoming connections now connects out to the library computer instead, so Windows Firewall and the macOS firewall need no change on the helping computer. Pairing still needs the same code confirmed on both computers. If both computers block PhotoFindry, one of them still needs a one-time Allow.

**Windows catches up from build 132.** Windows also receives the fixes released for Mac and Linux since build 132, including:

- From this build, updates and rollbacks keep the Windows sign-in task unchanged, and an update blocked by Windows application control keeps the working version.
- Local AI Search no longer pauses for memory while refining a whole-library search on Windows graphics cards.
- One-action **Retry tagging** in Needs attention, and the source-reconnection and search fixes described in the 0.3.0-beta.3 to beta.10 release notes.

**Clearer recovery messages.** If an update's recovery cannot read a retained program file, the message now names the file and why, for example that it is missing because security software removed it, and reports both versions when neither can be verified.

Windows program files are still not code-signed. Windows 11 Smart App Control can block them, and Microsoft Defender can remove one; rerunning the setup block restores verified files and keeps your library.

Updates preserve the library, originals, model paths, saved lenses and saved work. All inference stays on your computers; a household helper receives only the prepared preview of each photo it tags, over an encrypted connection between paired computers. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
