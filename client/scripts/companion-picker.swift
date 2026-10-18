import AppKit
import Foundation
import UniformTypeIdentifiers

// The installed companion owns this helper. The browser can request a fixed
// picker kind; it cannot supply executable code, a path, or a shell command.
func finish(_ value: [String: Any], code: Int32 = 0) -> Never {
    if let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([10]))
    }
    exit(code)
}

// Kept separate from the entry point so configuration can be qualified without
// presenting a chooser or activating the helper application.
func configureNativeOpenPanel(_ panel: NSOpenPanel, kind: String) {
    let guidance = [
        "model": "Choose the main vision-language model weights (.gguf), for example Qwen3.5-4B-…-Q4_K_M.gguf. The separate mmproj file goes under Vision adapter.",
        "projector": "Choose mmproj-….gguf from the same model release as your main model. This adapter lets it read images; it is not a chat template.",
        "reviewModel": "Choose the .gguf weights for the reviewer selected in PhotoFindry: Intern-Decision-4B-Q4_K_M.gguf or the supported CLEF Flash 9B Q8 model. Its separate mmproj file belongs under the reviewer's matching vision adapter.",
        "reviewProjector": "Choose the mmproj .gguf supplied for your selected Intern-Decision or CLEF Flash reviewer. For Intern-Decision, use mmproj-Intern-Decision-4B-F16.gguf. The reviewer and adapter must be a matching pair.",
        "reranker": "Choose a supported search reranker .gguf, for example qwen3-reranker-0.6b-q8_0.gguf. This is separate from your photo-description model.",
        "template": "Usually you do not need this. Choose only the model author's small .jinja or .txt template. Model weights and mmproj .gguf files do not belong here.",
        "backupOpen": "Select the whole .photofindrybackup or legacy .photorecallbackup folder, then choose Open. Do not select catalog.sqlite, manifest.json or another file inside it. A .photofindrysource.zip source archive uses Attach source archive instead.",
        "archiveOpen": "Choose a .photofindrysource.zip source archive to add it to this library. For a .photofindrybackup or legacy .photorecallbackup library folder, use Import library backup instead."
    ]
    if let message = guidance[kind] { panel.message = message }
    panel.allowsMultipleSelection = false
    panel.canChooseDirectories = ["folder", "sourceFolder", "backupOpen"].contains(kind)
    panel.canChooseFiles = !["folder", "sourceFolder"].contains(kind)
    // Match the established native backup picker. Registered backup packages
    // must remain selectable as a whole instead of exposing their internals.
    // Ordinary folders also stay selectable when the exporting Mac's package
    // type is not registered on this installation yet.
    if kind == "backupOpen" { panel.treatsFilePackagesAsDirectories = false }
    if ["model", "projector", "reviewModel", "reviewProjector", "reranker"].contains(kind) {
        panel.allowedContentTypes = [UTType(filenameExtension: "gguf") ?? .data]
    } else if kind == "template" {
        panel.allowedContentTypes = [UTType(filenameExtension: "jinja") ?? .text, .text]
    } else if kind == "archiveOpen" {
        panel.allowedContentTypes = [.zip]
    } else if kind == "screenshot" {
        panel.allowedContentTypes = [.png, .jpeg]
    }
}

// Native picker entry point.
if CommandLine.arguments.count == 3, ["reveal", "openPhoto"].contains(CommandLine.arguments[1]) {
    let url = URL(fileURLWithPath: CommandLine.arguments[2])
    guard CommandLine.arguments[2].hasPrefix("/"), FileManager.default.fileExists(atPath: url.path) else {
        finish(["error": "The original is unavailable."], code: 2)
    }
    if CommandLine.arguments[1] == "reveal" {
        NSWorkspace.shared.activateFileViewerSelecting([url]); finish(["opened": true])
    }
    let images = Set(["jpg", "jpeg", "png", "heic", "heif", "webp", "tif", "tiff", "bmp", "gif", "avif", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2"])
    guard images.contains(url.pathExtension.lowercased()) else { finish(["error": "Only original photographs can be opened."], code: 2) }
    finish(["opened": NSWorkspace.shared.open(url)])
}
let kinds = Set(["folder", "sourceFolder", "model", "projector", "reviewModel", "reviewProjector", "template", "reranker", "backupOpen", "backupSave", "archiveOpen", "archiveSave", "photoExportSave", "screenshot"])
guard CommandLine.arguments.count == 2, kinds.contains(CommandLine.arguments[1]) else {
    finish(["error": "Unknown picker kind."], code: 2)
}
let kind = CommandLine.arguments[1]
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
app.finishLaunching()
app.activate(ignoringOtherApps: true)

let titles = [
    "folder": "Choose a photo folder", "sourceFolder": "Choose originals folder", "model": "Choose your local vision model",
    "projector": "Choose the matching vision adapter (mmproj)", "template": "Choose an optional text chat template",
    "reviewModel": "Choose your photo detail reviewer (decision model)",
    "reviewProjector": "Choose the reviewer's matching vision adapter (mmproj)",
    "reranker": "Choose your local search model", "backupOpen": "Open a PhotoFindry library backup",
    "backupSave": "Export a PhotoFindry library backup", "archiveOpen": "Open a source archive",
    "archiveSave": "Export a source archive", "photoExportSave": "Save a copy of this cached preview", "screenshot": "Choose a feedback screenshot"
]

let selected: URL?
if kind.hasSuffix("Save") {
    let panel = NSSavePanel()
    panel.title = titles[kind]!
    panel.canCreateDirectories = true
    panel.isExtensionHidden = false
    panel.nameFieldStringValue = kind == "backupSave" ? "PhotoFindry.photofindrybackup"
        : kind == "archiveSave" ? "PhotoFindry.photofindrysource.zip" : "PhotoFindry-preview.jpg"
    if kind == "archiveSave" { panel.allowedContentTypes = [.zip] }
    if kind == "photoExportSave" { panel.allowedContentTypes = [.jpeg] }
    selected = panel.runModal() == .OK ? panel.url : nil
} else {
    let panel = NSOpenPanel()
    panel.title = titles[kind]!
    configureNativeOpenPanel(panel, kind: kind)
    selected = panel.runModal() == .OK ? panel.url : nil
}

guard let selected else { finish(["cancelled": true]) }
finish(["path": selected.path, "cancelled": false])
