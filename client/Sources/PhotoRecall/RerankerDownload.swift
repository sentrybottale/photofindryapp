import AppKit
import CryptoKit
import Darwin
import Foundation

/// Explicit, cancellable download. Only the pinned upstream bytes are adapted;
/// the output hash also verifies the unchanged tensor payload and photo template.
@MainActor
final class RerankerDownload: NSObject, ObservableObject, URLSessionDownloadDelegate {
    static let source = URL(string: "https://huggingface.co/ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF/resolve/a02f48bb4f057028298c21fa033da2b30d7742d5/qwen3-reranker-0.6b-q8_0.gguf")!
    @Published private(set) var running = false
    @Published private(set) var progress: Double = 0
    @Published private(set) var message = ""
    @Published private(set) var downloadedPath: String?
    // The CLI companion supplies the same pinned native engine explicitly.
    // App bundles retain their existing resource-relative default.
    var preparationEngine: URL?
    private var session: URLSession?
    private var download: URLSessionDownloadTask?
    private var validation: Task<Void, Never>?
    private var destination: URL?
    private var generation = UUID()

    func chooseDestinationAndStart() {
        guard !running else { return }
        let panel = NSSavePanel()
        panel.title = "Save the search reranker outside the app"
        panel.nameFieldStringValue = "qwen3-reranker-0.6b-q8_0-photofindry.gguf"
        panel.message = "Downloads about 639 MB from Hugging Face. Keep this file where PhotoFindry can find it."
        guard panel.runModal() == .OK, let url = panel.url else { return }
        guard !FileManager.default.fileExists(atPath: url.path) else {
            message = "Choose a new filename, or use Choose file to select the model already there."
            return
        }
        destination = url; generation = UUID(); downloadedPath = nil
        running = true; progress = 0; message = "Downloading search reranker…"
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 60
        configuration.timeoutIntervalForResource = 3600
        let session = URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
        self.session = session
        download = session.downloadTask(with: Self.source)
        download?.resume()
    }

    func cancel() {
        generation = UUID(); download?.cancel(); validation?.cancel()
        session?.invalidateAndCancel(); session = nil; download = nil
        running = false; message = "Download cancelled. Your selected model is unchanged."
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                               didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        if totalBytesWritten > 639_153_184 { downloadTask.cancel() }
        Task { @MainActor in
            guard self.download === downloadTask else { return }
            self.progress = min(1, Double(totalBytesWritten) / 639_153_184)
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask,
                               willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
                               completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(request.url?.scheme == "https" ? request : nil)
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        // URLSession deletes its temporary file on return from this callback.
        let staged = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".gguf")
        do {
            guard (downloadTask.response as? HTTPURLResponse)?.statusCode == 200 else {
                throw CocoaError(.fileReadUnknown)
            }
            try FileManager.default.moveItem(at: location, to: staged)
            Task { @MainActor in
                guard self.download === downloadTask, let destination = self.destination else {
                    try? FileManager.default.removeItem(at: staged); return
                }
                let generation = self.generation
                self.message = "Verifying model and preparing photo search…"
                self.download = nil
                self.validation = Task {
                    let engine = self.preparationEngine
                    let work = Task.detached(priority: .utility) { try RecommendedReranker.prepare(staged, destination: destination, engine: engine) }
                    let result = await withTaskCancellationHandler { await work.result } onCancel: { work.cancel() }
                    try? FileManager.default.removeItem(at: staged)
                    guard self.generation == generation else { return }
                    self.running = false; self.session?.finishTasksAndInvalidate(); self.session = nil
                    switch result {
                    case .success: self.downloadedPath = destination.path; self.message = "Download verified. Ready to select."
                    case .failure(let error): self.message = "Could not prepare the reranker: \(error.localizedDescription) Try again or choose a local file."
                    }
                }
            }
        } catch {
            Task { @MainActor in
                guard self.download === downloadTask else { return }
                self.running = false; self.download = nil
                self.message = "The model download failed. Check your connection and try again."
                self.session?.invalidateAndCancel(); self.session = nil
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let error else { return }
        Task { @MainActor in
            guard self.download === task else { return }
            self.running = false; self.download = nil
            self.message = "Download failed: \(error.localizedDescription) You can try again."
            self.session?.invalidateAndCancel(); self.session = nil
        }
    }
}

enum RecommendedReranker {
    static let sourceSHA = "22c9979ce4fbcdc5acdc310c6641c32797eff1aa980b8f7a2db8a8ea23429a48"
    static let outputSHA = "2f97b25e84538446c74641f33d2b0e0b982ed72a14cc7009046fa7dba09f85f3"
    private static func checksum(_ url: URL) throws -> String {
        let input = try FileHandle(forReadingFrom: url); defer { try? input.close() }
        var hash = SHA256()
        while let data = try input.read(upToCount: 1024 * 1024), !data.isEmpty {
            try Task.checkCancellation(); hash.update(data: data)
        }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }
    static func prepare(_ source: URL, destination: URL, engine: URL? = nil) throws {
        guard try checksum(source) == sourceSHA else { throw failure("Downloaded model checksum does not match the recommended version.") }
        try Task.checkCancellation()
        guard !FileManager.default.fileExists(atPath: destination.path) else {
            throw failure("The selected file already exists. Choose a new filename.")
        }
        let stage = destination.deletingLastPathComponent().appendingPathComponent(".reranker-\(UUID().uuidString).tmp")
        defer { try? FileManager.default.removeItem(at: stage) }
        guard let engine = engine ?? Bundle.main.resourceURL?.appendingPathComponent("runtime/PhotoEngine") else {
            throw failure("The app is missing its model preparation runtime. Reinstall PhotoFindry.")
        }
        try RerankerPreparationClient.prepare(source: source, staging: stage, engine: engine)
        guard try checksum(stage) == outputSHA else { throw failure("Prepared model verification failed.") }
        try Task.checkCancellation()
        // A same-directory move publishes atomically and never overwrites an existing file.
        try FileManager.default.moveItem(at: stage, to: destination)
    }
    private static func failure(_ message: String) -> NSError { NSError(domain: "PhotoFindry.ModelDownload", code: 1, userInfo: [NSLocalizedDescriptionKey: message]) }
}

/// The public download host passes only paths to its bundled, owned engine.
/// Model adaptation details never live in the application source.
enum RerankerPreparationClient {
    private struct Response: Decodable {
        struct Result: Decodable { let prepared: Bool; let outputSHA256: String; let outputBytes: UInt64 }
        struct EngineError: Decodable { let message: String }
        let id: Int
        let result: Result?
        let error: EngineError?
    }

    static func prepare(source: URL, staging: URL, engine: URL, timeout: TimeInterval = 300) throws {
        try Task.checkCancellation()
        guard FileManager.default.isExecutableFile(atPath: engine.path) else {
            throw failure("The app is missing its model preparation runtime. Reinstall PhotoFindry.")
        }
        let request = try JSONSerialization.data(withJSONObject: [
            "id": 1, "operation": "model.prepareRecommendedReranker",
            "params": ["sourcePath": source.path, "stagingPath": staging.path]
        ]) + Data([10])
        guard request.count <= 12_288 else { throw failure("The selected model path is too long.") }
        let process = Process(), input = Pipe(), output = Pipe()
        process.executableURL = engine
        process.arguments = ["--once"]
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin"]
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        let descriptor = output.fileHandleForReading.fileDescriptor
        guard fcntl(descriptor, F_SETFL, fcntl(descriptor, F_GETFL) | O_NONBLOCK) != -1 else {
            throw failure("Cannot connect to the model preparation runtime.")
        }
        defer {
            stop(process)
            try? input.fileHandleForWriting.close()
            try? output.fileHandleForReading.close()
        }
        try process.run()
        try input.fileHandleForWriting.write(contentsOf: request)
        try input.fileHandleForWriting.close()
        var response = Data(), buffer = [UInt8](repeating: 0, count: 4096)
        let deadline = Date().addingTimeInterval(timeout)
        func drain() throws {
            while true {
                let count = read(descriptor, &buffer, buffer.count)
                if count > 0 {
                    guard response.count + count <= 65_536 else { throw failure("The model preparation runtime returned too much data.") }
                    response.append(contentsOf: buffer.prefix(count))
                } else if count == 0 || errno == EAGAIN || errno == EWOULDBLOCK { return }
                else if errno != EINTR { throw failure("Cannot read the model preparation result.") }
            }
        }
        while process.isRunning {
            try Task.checkCancellation()
            guard Date() < deadline else { throw failure("Model preparation timed out. Try again.") }
            try drain()
            Thread.sleep(forTimeInterval: 0.02)
        }
        try drain()
        try Task.checkCancellation()
        guard process.terminationStatus == 0 else { throw failure("The model preparation runtime stopped unexpectedly.") }
        let decoded: Response
        do { decoded = try JSONDecoder().decode(Response.self, from: response) }
        catch { throw failure("The model preparation runtime returned an invalid result.") }
        guard decoded.id == 1 else { throw failure("The model preparation result did not match this request.") }
        if let error = decoded.error { throw failure(error.message) }
        guard let result = decoded.result, result.prepared, result.outputBytes > 0,
              result.outputSHA256 == RecommendedReranker.outputSHA else {
            throw failure("The prepared model does not match this version of PhotoFindry.")
        }
    }

    private static func stop(_ process: Process) {
        guard process.isRunning else { return }
        process.terminate()
        let deadline = Date().addingTimeInterval(2)
        while process.isRunning && Date() < deadline { Thread.sleep(forTimeInterval: 0.02) }
        if process.isRunning { kill(process.processIdentifier, SIGKILL) }
        process.waitUntilExit()
    }

    private static func failure(_ message: String) -> NSError {
        NSError(domain: "PhotoFindry.ModelDownload", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}
