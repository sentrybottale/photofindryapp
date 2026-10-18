# PhotoFindry 0.2.0-beta.15 — CLI lifecycle hotfix

The Mac application is now a one-time migration installer. It installs the
independent background companion, opens your local browser workspace and exits.
Normal use no longer needs a permanent Mac app or an open Terminal.

- The companion starts at login. Open your workspace from app.photofindry.com;
  the website receives no library authority.
- Settings → Background companion manages login startup, automatic update checks,
  signed updates and recovery. Checks do not silently install updates.
- Updates use the existing publisher key. Interrupted switches recover verified
  prior software without restoring an older library. The native opening helper
  offers Recover and open when needed.
- Uninstall disables autostart and moves the active program to Trash. Libraries,
  original photographs, model paths, backups and inactive recovery versions stay.
- If macOS needs permission to read original photos, Jobs offers Open Files.
  Choose the originals folder, then use Retry access. The operation times out
  safely instead of leaving the processing lane stuck.
- Existing library migration retains saved photos, tags, People, model paths and
  paused work. The retained native beta remains available if migration fails.

This is a focused hotfix. Inference policy, models and data schemas are unchanged. The private worker
and native helper now bound macOS original-file consent checks and preserve
permission-held work for explicit recovery. The MIT client remains open source; the Rust cores and private
implementation history remain proprietary. Official builds remain free.

Requires Apple silicon and macOS 14 or later; Core ML ranking requires macOS 15.
Community beta: ad-hoc Mac signatures plus publisher-authenticated artifacts,
not Apple Developer ID signing or notarization. Keep independent photo backups.
