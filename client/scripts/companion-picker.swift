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

// macOS 14 and later may refuse to activate an app that the companion starts
// in the background. A chooser shown at once then stays in an inactive window:
// macOS draws its contents in a separate panel service, so clicks select rows
// only in some places. macOS does consider activating an app that has its own
// window, so this helper first runs one: invisible at first, and when macOS
// activates it the chooser opens focused straight away. If activation has not
// happened after a moment, the window appears with a Choose button; the
// person's click on it activates this helper (the button accepts that first
// click), and the chooser then opens focused and fully usable.
final class FocusFirstClickButton: NSButton {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}
final class FocusPromptTarget: NSObject, NSWindowDelegate {
    @objc func choose(_ sender: Any?) { NSApp.stopModal(withCode: .OK) }
    @objc func cancel(_ sender: Any?) { NSApp.stopModal(withCode: .cancel) }
    func windowWillClose(_ notification: Notification) { NSApp.stopModal(withCode: .cancel) }
}
func focusSettle(_ app: NSApplication, seconds: TimeInterval) {
    let until = Date().addingTimeInterval(seconds)
    while !app.isActive && Date() < until { _ = RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
}
/// Returns false when the person cancelled the prompt.
func focusBeforeChooser(title: String, action: String) -> Bool {
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    app.finishLaunching()
    app.activate()
    focusSettle(app, seconds: 0.4)
    if app.isActive { return true }
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 440, height: 132), styleMask: [.titled, .closable], backing: .buffered, defer: false)
    window.title = "PhotoFindry"
    window.level = .floating
    window.isReleasedWhenClosed = false
    let target = FocusPromptTarget()
    window.delegate = target
    let heading = NSTextField(wrappingLabelWithString: title)
    heading.font = .boldSystemFont(ofSize: NSFont.systemFontSize + 1)
    let note = NSTextField(wrappingLabelWithString: "Your Mac opens its chooser after this click.")
    note.textColor = .secondaryLabelColor
    let cancel = FocusFirstClickButton(title: "Cancel", target: target, action: #selector(FocusPromptTarget.cancel(_:)))
    cancel.keyEquivalent = "\u{1b}"
    let choose = FocusFirstClickButton(title: action, target: target, action: #selector(FocusPromptTarget.choose(_:)))
    choose.keyEquivalent = "\r"
    choose.setAccessibilityIdentifier("photofindry-focus-choose")
    let buttons = NSStackView(views: [NSView(), cancel, choose])
    buttons.orientation = .horizontal
    let stack = NSStackView(views: [heading, note, buttons])
    stack.orientation = .vertical
    stack.alignment = .leading
    stack.spacing = 8
    stack.edgeInsets = NSEdgeInsets(top: 18, left: 20, bottom: 16, right: 20)
    stack.translatesAutoresizingMaskIntoConstraints = false
    let content = NSView()
    content.addSubview(stack)
    NSLayoutConstraint.activate([
        stack.leadingAnchor.constraint(equalTo: content.leadingAnchor), stack.trailingAnchor.constraint(equalTo: content.trailingAnchor),
        stack.topAnchor.constraint(equalTo: content.topAnchor), stack.bottomAnchor.constraint(equalTo: content.bottomAnchor),
        buttons.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -40), heading.widthAnchor.constraint(lessThanOrEqualToConstant: 400)
    ])
    window.contentView = content
    window.center()
    window.alphaValue = 0
    window.orderFrontRegardless()
    let shown = Date()
    let watcher = Timer(timeInterval: 0.05, repeats: true) { timer in
        if app.isActive && window.isKeyWindow && window.alphaValue == 0 {
            timer.invalidate()
            app.stopModal(withCode: .OK)
        } else if window.alphaValue == 0 && Date().timeIntervalSince(shown) > 0.6 {
            window.alphaValue = 1
        }
    }
    RunLoop.main.add(watcher, forMode: .modalPanel)
    RunLoop.main.add(watcher, forMode: .common)
    #if FOCUS_PROMPT_TESTS
    // Qualification builds only: report what the person would see, then
    // answer as if they chose. Release builds never answer for the person.
    focusPromptTestHook(window: window, active: app.isActive, choose: choose)
    #endif
    let answer = app.runModal(for: window)
    watcher.invalidate()
    window.delegate = nil
    window.orderOut(nil)
    guard answer == .OK else { return false }
    app.activate()
    focusSettle(app, seconds: 1)
    return true
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

let titles = [
    "folder": "Choose a photo folder", "sourceFolder": "Choose originals folder", "model": "Choose your local vision model",
    "projector": "Choose the matching vision adapter (mmproj)", "template": "Choose an optional text chat template",
    "reviewModel": "Choose your photo detail reviewer (decision model)",
    "reviewProjector": "Choose the reviewer's matching vision adapter (mmproj)",
    "reranker": "Choose your local search model", "backupOpen": "Open a PhotoFindry library backup",
    "backupSave": "Export a PhotoFindry library backup", "archiveOpen": "Open a source archive",
    "archiveSave": "Export a source archive", "photoExportSave": "Save a copy of this cached preview", "screenshot": "Choose a feedback screenshot"
]

guard focusBeforeChooser(title: titles[kind]!, action: kind.hasSuffix("Save") ? "Choose where to save…" : "Choose…") else { finish(["cancelled": true]) }
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
