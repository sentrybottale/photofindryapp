import AppKit
import SwiftUI
import UniformTypeIdentifiers

// SourceLabels.swift supplies the existing protocol, Vision reader, print view,
// camera permission flow and frame-dropping camera pipeline unchanged.
enum PhotoDesign {
    static let paper = Color(red: 0.98, green: 0.98, blue: 0.96)
    static let ink = Color(red: 0.08, green: 0.15, blue: 0.23)
    static let amber = Color(red: 0.60, green: 0.43, blue: 0.18)
}
func finishLabel(_ value: [String: Any], code: Int32 = 0) -> Never {
    if let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
        FileHandle.standardOutput.write(data); FileHandle.standardOutput.write(Data([10]))
    }
    exit(code)
}
final class LabelWindowDelegate: NSObject, NSWindowDelegate {
    func windowWillClose(_ notification: Notification) { finishLabel(["cancelled": true]) }
}
struct LabelScannerRoot: View {
    @State var visible = true
    var body: some View {
        Text("PhotoFindry archive scanner").padding(30)
            .sheet(isPresented: $visible, onDismiss: { finishLabel(["cancelled": true]) }) {
                SourceQRScanner { payload in
                    guard let id = SourceArchiveCode.archiveID(from: payload) else { finishLabel(["error": "Invalid archive label."], code: 2) }
                    finishLabel(["sourceId": id, "payload": payload])
                }
            }
    }
}
@main
struct CompanionLabels {
    @MainActor static func main() {
        guard let line = readLine(), line.utf8.count <= 8192,
              let bytes = line.data(using: .utf8),
              let input = try? JSONSerialization.jsonObject(with: bytes) as? [String: String],
              let operation = input["operation"] else { exit(2) }
        if operation == "selfTest" {
            let id = "12345678-1234-4567-89ab-123456789abc"
            guard let image = SourceArchiveCode.qrImage(for: id),
                  let code = try? SourceArchiveCode.read(image), SourceArchiveCode.archiveID(from: code) == id else { exit(2) }
            finishLabel(["roundTrip": true])
        }
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        app.finishLaunching()
        if operation == "scanImage" {
            app.activate(ignoringOtherApps: true)
            let panel = NSOpenPanel(); panel.allowedContentTypes = [.image]; panel.allowsMultipleSelection = false; panel.canChooseDirectories = false
            guard panel.runModal() == .OK, let url = panel.url else { finishLabel(["cancelled": true]) }
            do {
                guard let payload = try SourceArchiveCode.readImage(at: url), let id = SourceArchiveCode.archiveID(from: payload) else { finishLabel(["error": "No valid PhotoFindry archive label was found."], code: 2) }
                finishLabel(["sourceId": id, "payload": payload])
            } catch { finishLabel(["error": error.localizedDescription], code: 2) }
        }
        if operation == "scanCamera" {
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 480), styleMask: [.titled, .closable], backing: .buffered, defer: false)
            let delegate = LabelWindowDelegate(); window.delegate = delegate
            window.title = "PhotoFindry archive scanner"; window.contentView = NSHostingView(rootView: LabelScannerRoot())
            window.center(); window.makeKeyAndOrderFront(nil); app.activate(ignoringOtherApps: true)
            withExtendedLifetime(delegate) { app.run() }
            finishLabel(["cancelled": true])
        }
        guard let id = input["sourceId"], let payload = SourceArchiveCode.payload(for: id),
              let label = input["label"], label.utf8.count <= 1024,
              ["show", "print", "savePDF"].contains(operation) else { exit(2) }
        let source = SourceLabelIdentity(id: id, label: label)
        if operation == "show" {
            guard let image = SourceArchiveCode.qrImage(for: id), let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]), png.count <= 262144 else { exit(2) }
            finishLabel(["sourceId": id, "payload": payload, "pngBase64": png.base64EncodedString()])
        }
        app.activate(ignoringOtherApps: true)
        if operation == "print" {
            let info = NSPrintInfo.shared.copy() as! NSPrintInfo
            info.horizontalPagination = .fit; info.verticalPagination = .fit
            info.isHorizontallyCentered = true; info.isVerticallyCentered = true
            let printOperation = NSPrintOperation(view: SourceLabelPrintView(source: source), printInfo: info)
            printOperation.showsPrintPanel = true; printOperation.showsProgressPanel = true
            finishLabel(["printed": printOperation.run()])
        }
        let panel = NSSavePanel(); panel.allowedContentTypes = [.pdf]
        panel.nameFieldStringValue = "PhotoFindry-" + SourceArchiveCode.shortCode(for: id) + ".pdf"
        guard panel.runModal() == .OK, let url = panel.url else { finishLabel(["cancelled": true]) }
        do { try SourceLabelPrintView(source: source).pdfData().write(to: url, options: .atomic); finishLabel(["saved": true]) }
        catch { finishLabel(["error": "The label could not be saved."], code: 2) }
    }
}
