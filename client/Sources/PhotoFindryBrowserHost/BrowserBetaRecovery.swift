import Foundation

/// A user-selected, signed native beta is an alternative to browser handover.
/// Public links cannot select an application or trigger this action.
enum BrowserBetaRecovery {
    struct Candidate: Equatable {
        let url: URL
        let build: Int
        fileprivate let info: Data
        fileprivate let manifest: Data
    }

    static func preferredApplication(home: URL) -> URL {
        home.appendingPathComponent("Applications/PhotoFindry Beta.app", isDirectory: true)
    }

    private static func failure(_ message: String) -> BrowserHostFailure {
        .init(message: message, nextAction: "Choose your saved native PhotoFindry beta (build 108 or newer), or reinstall it from your trusted beta installer. Your library is not restored, replaced or retagged by this choice.")
    }

    private static func metadata(_ url: URL, directory: Bool, maximum: Int = 0) throws -> Data {
        let values = try url.resourceValues(forKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isSymbolicLink != true,
              directory ? values.isDirectory == true : values.isRegularFile == true else {
            throw failure("Choose a complete native PhotoFindry application, not a link or installer file.")
        }
        if directory { return Data() }
        guard let size = values.fileSize, size > 0, size <= maximum else {
            throw failure("This beta application has invalid or oversized identity information.")
        }
        let data = try Data(contentsOf: url)
        guard data.count <= maximum else { throw failure("This beta application changed while checking it.") }
        return data
    }

    static func candidate(at url: URL) throws -> Candidate {
        guard url.isFileURL, url.pathExtension == "app" else { throw failure("Choose the saved PhotoFindry beta application.") }
        _ = try metadata(url, directory: true)
        let info = try metadata(url.appendingPathComponent("Contents/Info.plist"), directory: false, maximum: 65_536)
        let manifest = try metadata(url.appendingPathComponent("Contents/Resources/runtime-manifest.json"), directory: false, maximum: 4 * 1024 * 1024)
        guard let plist = try PropertyListSerialization.propertyList(from: info, format: nil) as? [String: Any],
              plist["CFBundleIdentifier"] as? String == "com.sentrybottale.photorecall",
              plist["CFBundleExecutable"] as? String == "PhotoFindry",
              plist["CFBundleShortVersionString"] as? String == "0.2.0",
              let version = plist["CFBundleVersion"] as? String, let build = Int(version), build >= 108,
              let runtime = try JSONSerialization.jsonObject(with: manifest) as? [String: Any],
              let engine = runtime["engine"] as? [String: Any],
              engine["implementation"] as? String == "rust", engine["host"] as? String == "rust",
              engine["protocol"] as? Int == 1, engine["complete"] as? Bool == true else {
            throw failure("This application is not a compatible native PhotoFindry beta.")
        }
        for path in ["Contents/MacOS/PhotoFindry", "Contents/Resources/runtime/PhotoWorker", "Contents/Resources/runtime/PhotoEngine"] {
            let file = url.appendingPathComponent(path)
            let values = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            guard values.isRegularFile == true, values.isSymbolicLink != true,
                  FileManager.default.isExecutableFile(atPath: file.path) else {
                throw failure("The saved beta is incomplete. Choose or reinstall the complete application.")
            }
        }
        return Candidate(url: url, build: build, info: info, manifest: manifest)
    }

    @MainActor static func handoff(to candidate: Candidate, runner: any BrowserCommandRunning, updates: BrowserUpdates? = nil,
        prepare: () async throws -> Void, open: (URL) async throws -> Void) async throws {
        // Validate before draining any working companion. A bad choice must not
        // interrupt a user's current work, and no shell or model is involved.
        guard try Self.candidate(at: candidate.url) == candidate else { throw failure("The saved beta changed. Choose it again.") }
        let signature = try await runner.run(executable: URL(fileURLWithPath: "/usr/bin/codesign"),
            arguments: ["--verify", "--deep", "--strict", candidate.url.path], json: false, progress: { _ in })
        guard signature.exitCode == 0 else { throw failure("The saved beta's application signature could not be verified.") }
        guard try Self.candidate(at: candidate.url) == candidate else { throw failure("The saved beta changed during verification. Choose it again.") }
        // Sparkle can install on ordinary Quit, including before handover.
        // Switching to the beta must preserve that same package-idle gate.
        if let updates { try await updates.prepareForTermination(ordinary: prepare) }
        else { try await prepare() }
        try await open(candidate.url)
    }
}
