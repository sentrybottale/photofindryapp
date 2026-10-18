import SwiftUI
import AppKit
import AVFoundation
import CoreImage
import CoreImage.CIFilterBuiltins
import Vision
import ImageIO
import UniformTypeIdentifiers

struct SourceLabelIdentity: Identifiable, Equatable {
    let id: String
    let label: String
}

enum SourceArchiveCode {
    static func payload(for id: String) -> String? {
        guard let uuid = UUID(uuidString: id) else { return nil }
        return "photofindry://archive/v1/" + uuid.uuidString.lowercased()
    }

    static func archiveID(from payload: String) -> String? {
        guard payload.utf8.count <= 128, !payload.contains("%"),
              let parts = URLComponents(string: payload), parts.scheme == "photofindry", parts.host == "archive",
              parts.user == nil, parts.password == nil, parts.port == nil, parts.query == nil, parts.fragment == nil else { return nil }
        let path = parts.path.split(separator: "/", omittingEmptySubsequences: false)
        guard path.count == 3, path[0].isEmpty, path[1] == "v1", let uuid = UUID(uuidString: String(path[2])),
              payload == "photofindry://archive/v1/" + String(path[2]) else { return nil }
        return uuid.uuidString.lowercased()
    }

    static func shortCode(for id: String) -> String { String(id.uppercased().prefix(8)) }

    static func qrImage(for id: String) -> CGImage? {
        guard let payload = payload(for: id) else { return nil }
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(payload.utf8)
        filter.correctionLevel = "M"
        guard let qr = filter.outputImage else { return nil }
        // Four modules of quiet space are part of the generated asset, so an
        // exported or tightly cropped label remains readable.
        let extent = qr.extent.insetBy(dx: -4, dy: -4)
        let background = CIImage(color: .white).cropped(to: extent)
        let image = qr.composited(over: background).transformed(by: CGAffineTransform(translationX: 4, y: 4)).transformed(by: CGAffineTransform(scaleX: 8, y: 8))
        return CIContext(options: [.useSoftwareRenderer: false]).createCGImage(image, from: image.extent)
    }

    static func read(_ image: CGImage) throws -> String? {
        let request = VNDetectBarcodesRequest()
        request.symbologies = [.qr]
        try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
        return request.results?.compactMap(\.payloadStringValue).first { archiveID(from: $0) != nil }
    }

    static func readImage(at url: URL) throws -> String? {
        let values = try url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
        guard values.isRegularFile == true, let size = values.fileSize, size > 0, size <= 32 * 1024 * 1024,
              let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true, kCGImageSourceThumbnailMaxPixelSize: 4096] as CFDictionary) else {
            throw SourceLabelError.message("Choose a readable QR image no larger than 32 MB.")
        }
        return try read(image)
    }
}

enum SourceLabelError: LocalizedError {
    case message(String)
    var errorDescription: String? { switch self { case .message(let message): return message } }
}

@MainActor
final class SourceLabelPrintView: NSView {
    let source: SourceLabelIdentity
    init(source: SourceLabelIdentity) {
        self.source = source
        super.init(frame: NSRect(x: 0, y: 0, width: 396, height: 180))
    }
    required init?(coder: NSCoder) { fatalError("Use init(source:)") }
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        NSColor.white.setFill(); bounds.fill()
        let navy = NSColor(calibratedRed: 0.08, green: 0.15, blue: 0.23, alpha: 1)
        if let qr = SourceArchiveCode.qrImage(for: source.id) {
            NSGraphicsContext.current?.imageInterpolation = .none
            NSImage(cgImage: qr, size: NSSize(width: qr.width, height: qr.height))
                .draw(in: NSRect(x: 10, y: 16, width: 148, height: 148))
        }
        let paragraph = NSMutableParagraphStyle(); paragraph.lineBreakMode = .byTruncatingTail
        ("PhotoFindry" as NSString).draw(in: NSRect(x: 166, y: 24, width: 216, height: 20), withAttributes: [.font: NSFont.systemFont(ofSize: 11, weight: .medium), .foregroundColor: navy])
        (String(source.label.prefix(160)) as NSString).draw(in: NSRect(x: 166, y: 50, width: 216, height: 58), withAttributes: [.font: NSFont.systemFont(ofSize: 20, weight: .semibold), .foregroundColor: navy, .paragraphStyle: paragraph])
        (SourceArchiveCode.shortCode(for: source.id) as NSString).draw(in: NSRect(x: 166, y: 119, width: 216, height: 22), withAttributes: [.font: NSFont.monospacedSystemFont(ofSize: 16, weight: .medium), .foregroundColor: navy])
        ("Scan to browse your saved archive" as NSString).draw(in: NSRect(x: 166, y: 147, width: 216, height: 18), withAttributes: [.font: NSFont.systemFont(ofSize: 10), .foregroundColor: navy])
    }
    func pdfData() -> Data { dataWithPDF(inside: bounds) }
}

struct SourceLabelView: View {
    let source: SourceLabelIdentity
    @Environment(\.dismiss) private var dismiss
    @State private var error: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack { Text("Archive label").font(.title2.bold()); Spacer(); Button("Done") { dismiss() }.keyboardShortcut(.cancelAction) }
            HStack(spacing: 22) {
                if let qr = SourceArchiveCode.qrImage(for: source.id) {
                    Image(decorative: qr, scale: 1).interpolation(.none).resizable().frame(width: 164, height: 164).accessibilityLabel("QR code for \(source.label)")
                }
                VStack(alignment: .leading, spacing: 10) {
                    Text(source.label).font(.title3.bold()).lineLimit(3)
                    Text(SourceArchiveCode.shortCode(for: source.id)).font(.system(.title3, design: .monospaced)).textSelection(.enabled)
                    Text("Scan this label to browse saved photographs before connecting the drive.").font(.callout).foregroundStyle(.secondary)
                }
            }.padding(14).background(.white, in: RoundedRectangle(cornerRadius: 12))
            Text("The QR contains only an archive ID. It works with a library that includes this source; no filenames or photographs are stored in the code.").font(.caption).foregroundStyle(.secondary)
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
            HStack {
                Button("Print label…", systemImage: "printer") { printLabel() }
                Button("Save PDF…", systemImage: "square.and.arrow.down") { savePDF() }
            }.disabled(SourceArchiveCode.payload(for: source.id) == nil)
        }.padding(24).frame(width: 540).background(PhotoDesign.paper).foregroundStyle(PhotoDesign.ink).tint(PhotoDesign.amber)
    }
    private func printLabel() {
        let info = NSPrintInfo.shared.copy() as! NSPrintInfo
        info.horizontalPagination = .fit; info.verticalPagination = .fit
        info.isHorizontallyCentered = true; info.isVerticallyCentered = true
        let operation = NSPrintOperation(view: SourceLabelPrintView(source: source), printInfo: info)
        operation.showsPrintPanel = true; operation.showsProgressPanel = true
        operation.run()
    }
    private func savePDF() {
        let panel = NSSavePanel(); panel.allowedContentTypes = [.pdf]
        panel.nameFieldStringValue = "PhotoFindry-" + SourceArchiveCode.shortCode(for: source.id) + ".pdf"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do { try SourceLabelPrintView(source: source).pdfData().write(to: url, options: .atomic) }
        catch { self.error = error.localizedDescription }
    }
}

enum SourceCameraPermission {
    case ready, ask, denied
    static func state(_ status: AVAuthorizationStatus) -> Self {
        switch status { case .authorized: return .ready; case .notDetermined: return .ask; default: return .denied }
    }
}

@MainActor
final class SourceCameraAuthorization {
    private let status: () -> AVAuthorizationStatus
    private let request: () async -> Bool
    init(status: @escaping () -> AVAuthorizationStatus = { AVCaptureDevice.authorizationStatus(for: .video) },
         request: @escaping () async -> Bool = { await AVCaptureDevice.requestAccess(for: .video) }) {
        self.status = status; self.request = request
    }
    func authorize() async -> Bool {
        switch SourceCameraPermission.state(status()) {
        case .ready: return true
        case .denied: return false
        case .ask: return await request()
        }
    }
}

/// Capture and Vision stay on one serial queue. Frames are dropped while busy,
/// never saved, and no microphone input or network service is configured.
private final class SourceCameraPipeline: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate, @unchecked Sendable {
    let session = AVCaptureSession()
    let queue = DispatchQueue(label: "PhotoFindry.archive-qr-camera", qos: .userInitiated)
    var onPayload: ((String) -> Void)?
    var onFailure: ((String) -> Void)?
    private var lastFrame: CFTimeInterval = 0
    private var stopped = true
    private var output: AVCaptureVideoDataOutput?
    func start(onPayload: @escaping (String) -> Void, onFailure: @escaping (String) -> Void) {
        queue.async { [self] in
            self.onPayload = onPayload; self.onFailure = onFailure
            do {
                stopped = false
                if session.inputs.isEmpty {
                    guard let camera = AVCaptureDevice.default(for: .video) else { throw SourceLabelError.message("No camera is available. Choose a QR image or enter the archive ID instead.") }
                    let input = try AVCaptureDeviceInput(device: camera)
                    session.beginConfiguration()
                    defer { session.commitConfiguration() }
                    session.sessionPreset = .medium
                    guard session.canAddInput(input) else { throw SourceLabelError.message("This camera is unavailable.") }
                    session.addInput(input)
                    let output = AVCaptureVideoDataOutput(); output.alwaysDiscardsLateVideoFrames = true
                    output.setSampleBufferDelegate(self, queue: queue)
                    guard session.canAddOutput(output) else { throw SourceLabelError.message("The camera cannot provide QR images.") }
                    session.addOutput(output); self.output = output
                }
                session.startRunning()
            } catch {
                stopped = true
                session.beginConfiguration()
                for input in session.inputs { session.removeInput(input) }
                for output in session.outputs { session.removeOutput(output) }
                session.commitConfiguration()
                self.output = nil
                onFailure(error.localizedDescription)
            }
        }
    }
    func stop() { queue.async { [self] in stopped = true; session.stopRunning(); onPayload = nil; onFailure = nil } }
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard !stopped else { return }
        let now = CACurrentMediaTime()
        guard now - lastFrame >= 0.2, let buffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        lastFrame = now
        autoreleasepool {
            let request = VNDetectBarcodesRequest(); request.symbologies = [.qr]
            do {
                try VNImageRequestHandler(cvPixelBuffer: buffer, options: [:]).perform([request])
                guard let payload = request.results?.compactMap(\.payloadStringValue).first(where: { SourceArchiveCode.archiveID(from: $0) != nil }) else { return }
                stopped = true
                // Stop after this output callback returns, so synchronous
                // capture shutdown cannot wait on its own frame delivery.
                queue.async { [self] in
                    session.stopRunning()
                    onPayload?(payload)
                    onPayload = nil; onFailure = nil
                }
            } catch { /* An unreadable frame is expected; try the next frame. */ }
        }
    }
}

@MainActor
final class SourceCameraScanner: ObservableObject {
    @Published private(set) var running = false
    @Published private(set) var message = "Camera images stay on this Mac and are not saved."
    private let pipeline = SourceCameraPipeline()
    private let authorization: SourceCameraAuthorization
    private var generation = UUID()
    private var permissionPending = false
    var session: AVCaptureSession { pipeline.session }
    init(authorization: SourceCameraAuthorization? = nil) { self.authorization = authorization ?? SourceCameraAuthorization() }
    func start(onPayload: @escaping (String) -> Void) {
        guard !running, !permissionPending else { return }
        generation = UUID(); let expected = generation
        permissionPending = true
        Task { [weak self] in
            guard let self else { return }
            let granted = await authorization.authorize()
            guard generation == expected else { return }
            permissionPending = false
            guard granted else { message = "Camera access is off. Choose a QR image, enter the archive ID, or enable the camera in System Settings."; return }
            message = "Hold a PhotoFindry archive label in front of the camera."
            running = true
            pipeline.start(onPayload: { [weak self] payload in
                Task { @MainActor in guard let self, self.generation == expected else { return }; self.running = false; onPayload(payload) }
            }, onFailure: { [weak self] error in
                Task { @MainActor in guard let self, self.generation == expected else { return }; self.running = false; self.message = error }
            })
        }
    }
    func stop() { generation = UUID(); permissionPending = false; running = false; pipeline.stop() }
}

private struct SourceCameraPreview: NSViewRepresentable {
    let session: AVCaptureSession
    func makeNSView(context: Context) -> CameraView { CameraView(session: session) }
    func updateNSView(_ nsView: CameraView, context: Context) { }
    final class CameraView: NSView {
        let preview: AVCaptureVideoPreviewLayer
        init(session: AVCaptureSession) {
            preview = AVCaptureVideoPreviewLayer(session: session)
            super.init(frame: .zero)
            wantsLayer = true; layer = CALayer(); preview.videoGravity = .resizeAspectFill; layer?.addSublayer(preview)
        }
        required init?(coder: NSCoder) { fatalError("Use init(session:)") }
        override func layout() { super.layout(); preview.frame = bounds }
    }
}

struct SourceQRScanner: View {
    let onPayload: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @StateObject private var camera = SourceCameraScanner()
    @State private var enteredID = ""
    @State private var error: String?
    @State private var readingImage = false
    @State private var visible = true
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack { Text("Scan an archive label").font(.title2.bold()); Spacer(); Button("Done") { camera.stop(); dismiss() }.keyboardShortcut(.cancelAction) }
            if camera.running {
                SourceCameraPreview(session: camera.session).frame(height: 270).clipShape(RoundedRectangle(cornerRadius: 12))
            } else {
                Image(systemName: "qrcode.viewfinder").font(.system(size: 66, weight: .light)).frame(maxWidth: .infinity).frame(height: 130).foregroundStyle(.secondary)
            }
            Text(camera.message).font(.callout).foregroundStyle(.secondary)
            HStack {
                Button(camera.running ? "Stop camera" : "Start camera", systemImage: camera.running ? "stop.circle" : "camera") {
                    if camera.running { camera.stop() } else { error = nil; camera.start(onPayload: found) }
                }
                Button("Choose QR image…", systemImage: "photo") { chooseImage() }.disabled(readingImage)
                if readingImage { ProgressView().controlSize(.small) }
            }
            Divider()
            Text("Or enter the full archive ID").font(.caption).foregroundStyle(.secondary)
            HStack {
                TextField("Archive UUID or PhotoFindry QR link", text: $enteredID).textFieldStyle(.roundedBorder).onSubmit(openEntered)
                Button("Open archive", action: openEntered).disabled(enteredID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }.padding(24).frame(width: 560).background(PhotoDesign.paper).foregroundStyle(PhotoDesign.ink).tint(PhotoDesign.amber)
            .onAppear { visible = true }
            .onDisappear { visible = false; camera.stop() }
    }
    private func found(_ payload: String) { camera.stop(); onPayload(payload); dismiss() }
    private func openEntered() {
        let input = enteredID.trimmingCharacters(in: .whitespacesAndNewlines)
        if let payload = SourceArchiveCode.payload(for: input) { found(payload) }
        else if SourceArchiveCode.archiveID(from: input) != nil { found(input) }
        else { error = "Enter a complete PhotoFindry archive UUID or QR link." }
    }
    private func chooseImage() {
        let panel = NSOpenPanel(); panel.allowedContentTypes = [.image]; panel.allowsMultipleSelection = false; panel.canChooseDirectories = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        camera.stop(); readingImage = true; error = nil
        Task {
            do {
                let payload = try await Task.detached(priority: .userInitiated) { try SourceArchiveCode.readImage(at: url) }.value
                readingImage = false
                guard visible else { return }
                if let payload { found(payload) } else { error = "This image has no valid PhotoFindry archive label." }
            } catch { readingImage = false; self.error = error.localizedDescription }
        }
    }
}
