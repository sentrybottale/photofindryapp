'use strict';

// Exercise the production AppKit configuration without showing a chooser,
// activating PhotoFindry or returning any path capability.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

test('native import pickers select whole current and legacy backups and keep source ZIPs separate', { skip: process.platform !== 'darwin' }, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pf-picker-config-'));
  try {
    const original = fs.readFileSync(path.join(__dirname, '../scripts/companion-picker.swift'), 'utf8');
    const marker = '// Native picker entry point.';
    assert.equal(original.split(marker).length, 2);
    const driver = String.raw`
let application = NSApplication.shared
application.setActivationPolicy(.prohibited)
let backup = NSOpenPanel()
// A prior/system package preference must not turn backups into browsable
// internals. Test the explicit production setting rather than its default.
backup.treatsFilePackagesAsDirectories = true
configureNativeOpenPanel(backup, kind: "backupOpen")
assert(backup.canChooseDirectories && backup.canChooseFiles)
assert(!backup.treatsFilePackagesAsDirectories)
assert(!backup.allowsMultipleSelection)
assert(backup.allowedContentTypes.isEmpty)
assert(backup.message.contains(".photofindrybackup"))
assert(backup.message.contains(".photorecallbackup"))
assert(backup.message.contains("whole"))
assert(backup.message.contains("manifest.json"))
let archive = NSOpenPanel()
configureNativeOpenPanel(archive, kind: "archiveOpen")
assert(!archive.canChooseDirectories && archive.canChooseFiles)
assert(!archive.allowsMultipleSelection)
assert(archive.allowedContentTypes.map(\.identifier) == [UTType.zip.identifier])
assert(archive.message.contains("Import library backup"))
let folder = NSOpenPanel()
configureNativeOpenPanel(folder, kind: "folder")
assert(folder.canChooseDirectories && !folder.canChooseFiles)
assert(folder.allowedContentTypes.isEmpty)
let originals = NSOpenPanel()
configureNativeOpenPanel(originals, kind: "sourceFolder")
assert(originals.canChooseDirectories && !originals.canChooseFiles)
assert(!originals.allowsMultipleSelection && originals.allowedContentTypes.isEmpty)
assert(originals.message.isEmpty)
for kind in ["model", "projector", "reviewModel", "reviewProjector", "reranker"] {
    let panel = NSOpenPanel()
    configureNativeOpenPanel(panel, kind: kind)
    assert(!panel.canChooseDirectories && panel.canChooseFiles)
    assert(!panel.allowsMultipleSelection)
    assert(panel.allowedContentTypes == [UTType(filenameExtension: "gguf") ?? .data])
}
print("Native picker configuration passed; no chooser was presented.")
`;
    const source = path.join(root, 'main.swift'), binary = path.join(root, 'picker-config');
    fs.writeFileSync(source, original.slice(0, original.indexOf(marker)) + driver);
    const compiled = spawnSync('/usr/bin/xcrun', ['swiftc', '-framework', 'AppKit', '-framework', 'UniformTypeIdentifiers', source, '-o', binary], { encoding: 'utf8', timeout: 30000 });
    assert.equal(compiled.status, 0, compiled.stderr);
    const result = spawnSync('/bin/launchctl', ['asuser', String(process.getuid()), binary], { encoding: 'utf8', timeout: 15000 });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), 'Native picker configuration passed; no chooser was presented.');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});
