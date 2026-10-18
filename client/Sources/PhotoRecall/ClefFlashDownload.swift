import AppKit
import CryptoKit
import Foundation

struct ClefFlashFiles: Equatable, Sendable {
    let model: String
    let projector: String
}

struct ClefFlashAsset: Sendable {
    let filename: String
    let label: String
    let sha256: String
    let bytes: Int64

    var source: URL {
        URL(string: "https://registry.ollama.ai/v2/library/clef-flash/blobs/sha256:\(sha256)")!
    }
}

/// Pinned upstream files are copied unchanged. The licence travels with the weights.
enum RecommendedClefFlash {
    static let assets = [
        ClefFlashAsset(filename: "clef-flash-9b-q8_0.gguf", label: "CLEF Flash model",
                       sha256: "c9ac3afdf0b5928b00a5762aed44a338a0096af952194ccdedecdfd46bb92c33", bytes: 10_014_561_856),
        ClefFlashAsset(filename: "clef-flash-vision.gguf", label: "CLEF Flash vision projector",
                       sha256: "c8a6e9e00e1baf6433c4874dbc73bed2a56e887bcbb582fffcb00b533bc638dd", bytes: 918_165_984),
        ClefFlashAsset(filename: "CLEF-Flash-LICENSE.txt", label: "CLEF Flash licence",
                       sha256: "50cbab8a892c5f2993b8c7351a99182507472def3b1374558308605d99b86b32", bytes: 11_343)
    ]
    static let totalBytes = assets.reduce(Int64(0)) { $0 + $1.bytes }

    // This CDN host is the HTTPS redirect issued by the pinned official registry.
    // A changed upstream host requires another provenance review, not a broad wildcard.
    static func permitsDownloadURL(_ url: URL?) -> Bool {
        guard let url, url.scheme?.lowercased() == "https", url.user == nil, url.password == nil,
              url.port == nil || url.port == 443, let host = url.host?.lowercased() else { return false }
        return host == "registry.ollama.ai" || host == "dd20bb891979d25aebc8bec07b2b3bbc.r2.cloudflarestorage.com"
    }

    static func existingFileIsVerified(_ url: URL, asset: ClefFlashAsset) throws -> Bool {
        let values: URLResourceValues
        do { values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]) }
        catch let error as NSError where error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoSuchFileError { return false }
        catch { throw error }
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              Int64(values.fileSize ?? -1) == asset.bytes else {
            throw failure("\(asset.filename) already exists and is not the pinned file. Choose another folder or select your existing files manually.")
        }
        let input = try FileHandle(forReadingFrom: url)
        defer { try? input.close() }
        var hash = SHA256(), readBytes: Int64 = 0
        while let data = try input.read(upToCount: 1024 * 1024), !data.isEmpty {
            try Task.checkCancellation()
            readBytes += Int64(data.count)
            guard readBytes <= asset.bytes else {
                throw failure("\(asset.filename) changed while being verified. Choose another folder or try again.")
            }
            hash.update(data: data)
        }
        try Task.checkCancellation()
        guard readBytes == asset.bytes, digest(hash) == asset.sha256 else {
            throw failure("\(asset.filename) already exists with a different checksum. Choose another folder or select your existing files manually.")
        }
        return true
    }

    /// Publish a fully verified file without overwriting any destination.
    /// Streaming the copy keeps memory bounded and cancellation responsive across volumes.
    static func publish(_ source: URL, destination: URL, asset: ClefFlashAsset) throws {
        try Task.checkCancellation()
        if try existingFileIsVerified(destination, asset: asset) { return }
        let stage = destination.deletingLastPathComponent().appendingPathComponent(".clef-flash-\(UUID().uuidString).tmp")
        defer { try? FileManager.default.removeItem(at: stage) }
        guard FileManager.default.createFile(atPath: stage.path, contents: nil) else {
            throw failure("Cannot save CLEF Flash in the selected folder.")
        }
        let input = try FileHandle(forReadingFrom: source)
        defer { try? input.close() }
        let output = try FileHandle(forWritingTo: stage)
        defer { try? output.close() }
        var hash = SHA256(), written: Int64 = 0
        while let data = try input.read(upToCount: 1024 * 1024), !data.isEmpty {
            try Task.checkCancellation()
            written += Int64(data.count)
            guard written <= asset.bytes else { throw failure("The \(asset.label) download is larger than its pinned version.") }
            hash.update(data: data)
            try output.write(contentsOf: data)
        }
        guard written == asset.bytes, digest(hash) == asset.sha256 else {
            throw failure("The \(asset.label) download does not match its pinned checksum. Try again.")
        }
        try output.synchronize()
        try output.close()
        try Task.checkCancellation()
        // A same-directory move is atomic and fails if another file appeared meanwhile.
        try FileManager.default.moveItem(at: stage, to: destination)
    }

    private static func digest(_ hash: SHA256) -> String {
        hash.finalize().map { String(format: "%02x", $0) }.joined()
    }

    static func failure(_ message: String) -> NSError {
        NSError(domain: "PhotoFindry.ClefFlashDownload", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}

/// Explicit download only: completion never changes selected models or enables review.
@MainActor
final class ClefFlashDownload: NSObject, ObservableObject, URLSessionDownloadDelegate {
    @Published private(set) var running = false
    @Published private(set) var progress: Double = 0
    @Published private(set) var message = ""
    @Published private(set) var downloadedFiles: ClefFlashFiles?
    private var session: URLSession?
    private var download: URLSessionDownloadTask?
    private var validation: Task<Void, Never>?
    private var destination: URL?
    private var generation = UUID()
    private var index = 0
    private var existing: [Bool] = []
    private var completedBytes: Int64 = 0

    func chooseDestinationAndStart() {
        guard !running else { return }
        let panel = NSOpenPanel()
        panel.title = "Save CLEF Flash outside the app"
        panel.message = "Downloads about 10.9 GB from Ollama, including the vision projector and licence. Choose a model folder. This Q8 photo detail reviewer requires at least 24 GB Mac memory."
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let folder = panel.url else { return }
        start(in: folder)
    }

    func start(in folder: URL) {
        guard !running else { return }
        generation = UUID()
        let identity = generation
        destination = folder
        downloadedFiles = nil
        running = true
        progress = 0
        completedBytes = 0
        index = 0
        message = "Checking the selected model folder…"
        validation = Task {
            let work = Task.detached(priority: .utility) {
                let values = try folder.resourceValues(forKeys: [.isDirectoryKey])
                guard values.isDirectory == true else { throw RecommendedClefFlash.failure("Choose a folder for the model files.") }
                return try RecommendedClefFlash.assets.map { asset in
                    try RecommendedClefFlash.existingFileIsVerified(folder.appendingPathComponent(asset.filename), asset: asset)
                }
            }
            let result = await withTaskCancellationHandler { await work.result } onCancel: { work.cancel() }
            guard generation == identity else { return }
            validation = nil
            switch result {
            case .success(let present):
                existing = present
                let configuration = URLSessionConfiguration.ephemeral
                configuration.timeoutIntervalForRequest = 60
                configuration.timeoutIntervalForResource = 14_400
                session = URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
                continueDownload()
            case .failure(let error): fail(error)
            }
        }
    }

    func cancel() {
        generation = UUID()
        download?.cancel()
        validation?.cancel()
        session?.invalidateAndCancel()
        session = nil
        download = nil
        validation = nil
        running = false
        message = "Download cancelled. Verified files already saved remain in your chosen folder. Model selection is unchanged."
    }

    private func continueDownload() {
        guard running, let destination else { return }
        while index < RecommendedClefFlash.assets.count, existing[index] {
            completedBytes += RecommendedClefFlash.assets[index].bytes
            index += 1
        }
        progress = Double(completedBytes) / Double(RecommendedClefFlash.totalBytes)
        guard index < RecommendedClefFlash.assets.count else {
            running = false
            downloadedFiles = ClefFlashFiles(model: destination.appendingPathComponent(RecommendedClefFlash.assets[0].filename).path,
                                            projector: destination.appendingPathComponent(RecommendedClefFlash.assets[1].filename).path)
            message = "Model, vision projector and licence verified. Use the files below to select them."
            session?.finishTasksAndInvalidate()
            session = nil
            return
        }
        let asset = RecommendedClefFlash.assets[index]
        message = "Downloading \(asset.label)…"
        download = session?.downloadTask(with: asset.source)
        download?.taskDescription = String(index)
        download?.resume()
    }

    private func fail(_ error: Error) {
        running = false
        download = nil
        validation = nil
        session?.invalidateAndCancel()
        session = nil
        message = "Download failed: \(error.localizedDescription) Your model selection is unchanged."
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                               didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard let index = Int(downloadTask.taskDescription ?? ""), RecommendedClefFlash.assets.indices.contains(index) else {
            downloadTask.cancel(); return
        }
        if totalBytesWritten > RecommendedClefFlash.assets[index].bytes { downloadTask.cancel() }
        Task { @MainActor in
            guard self.download === downloadTask else { return }
            self.progress = min(1, Double(self.completedBytes + totalBytesWritten) / Double(RecommendedClefFlash.totalBytes))
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask,
                               willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
                               completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(RecommendedClefFlash.permitsDownloadURL(request.url) ? request : nil)
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        let staged = FileManager.default.temporaryDirectory.appendingPathComponent("clef-flash-\(UUID().uuidString).download")
        do {
            guard (downloadTask.response as? HTTPURLResponse)?.statusCode == 200,
                  RecommendedClefFlash.permitsDownloadURL(downloadTask.response?.url) else {
                throw RecommendedClefFlash.failure("The official model host returned an unsupported response or redirect.")
            }
            try FileManager.default.moveItem(at: location, to: staged)
            Task { @MainActor in
                guard self.download === downloadTask, let destination = self.destination else {
                    try? FileManager.default.removeItem(at: staged); return
                }
                let identity = self.generation
                let asset = RecommendedClefFlash.assets[self.index]
                self.message = "Verifying and saving \(asset.label)…"
                self.download = nil
                self.validation = Task {
                    let work = Task.detached(priority: .utility) {
                        defer { try? FileManager.default.removeItem(at: staged) }
                        try RecommendedClefFlash.publish(staged, destination: destination.appendingPathComponent(asset.filename), asset: asset)
                    }
                    let result = await withTaskCancellationHandler { await work.result } onCancel: { work.cancel() }
                    guard self.generation == identity else { return }
                    self.validation = nil
                    switch result {
                    case .success:
                        self.completedBytes += asset.bytes
                        self.index += 1
                        self.continueDownload()
                    case .failure(let error): self.fail(error)
                    }
                }
            }
        } catch {
            try? FileManager.default.removeItem(at: staged)
            Task { @MainActor in
                guard self.download === downloadTask else { return }
                self.fail(error)
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let error else { return }
        Task { @MainActor in
            guard self.download === task else { return }
            self.fail(error)
        }
    }
}
