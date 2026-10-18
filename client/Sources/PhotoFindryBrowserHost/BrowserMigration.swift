import Foundation

struct BrowserHostFailure: LocalizedError, Equatable {
    let message: String
    let nextAction: String
    var errorDescription: String? { message + "\n\n" + nextAction }
    static let invalidReply = Self(message: "PhotoFindry could not verify the companion’s response.",
        nextAction: "Choose Try again. If this repeats, reinstall the verified PhotoFindry application. Your photographs have not been changed.")
}

enum BrowserLaunchRequest: Equatable {
    case workspace
    case archive(String)
    init?(url: URL) {
        let value = url.absoluteString
        if value == "photofindry-local://open" { self = .workspace; return }
        let prefix = "photofindry://archive/v1/"
        guard value.hasPrefix(prefix) else { return nil }
        let text = String(value.dropFirst(prefix.count))
        let bytes = Array(text.utf8)
        guard bytes.count == 36, bytes.enumerated().allSatisfy({ offset, byte in
            [8, 13, 18, 23].contains(offset) ? byte == 45 :
                (48...57).contains(byte) || (65...70).contains(byte) || (97...102).contains(byte)
        }) else { return nil }
        self = .archive(text.lowercased())
    }
    var arguments: [String] {
        switch self { case .workspace: return ["open"]; case .archive(let id): return ["open-archive", id] }
    }
}

struct BrowserHostPaths: Equatable {
    let executable: URL
    let runtime: URL
    let library: URL
    let rollbackDirectory: URL
    let installation: URL
    let launchState: URL
    init(resources: URL, home: URL) {
        executable = resources.appendingPathComponent("companion/bin/photofindry")
        runtime = resources.appendingPathComponent("companion/runtime", isDirectory: true)
        library = home.appendingPathComponent("Library/Application Support/PhotoRecall", isDirectory: true)
        installation = home.appendingPathComponent("Library/Application Support/PhotoFindry Companion", isDirectory: true)
        launchState = library.appendingPathComponent("companion-launch", isDirectory: true)
        rollbackDirectory = home.appendingPathComponent("Library/Application Support/PhotoFindry Migration Backups", isDirectory: true)
    }
    func migrationArguments(rollback: URL) -> [String] {
        ["lifecycle", "install", "--install-root", installation.path, "--launch-state", launchState.path, "--data-dir", library.path, "--release", runtime.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("cli-release.json").path, "--rollback-copy", rollback.path]
    }
}

struct BrowserMigrationProgress: Decodable, Equatable {
    let event: String
    let stage: String
    let message: String
    var isValid: Bool {
        event == "progress" && ["checking", "backup", "opening", "registering"].contains(stage) &&
            !message.isEmpty && message.utf8.count <= 4096
    }
}

struct BrowserMigrationReply: Decodable {
    let ok: Bool
    let state: String?
    let message: String?
    let nextAction: String?
    let libraryId: String?
    let rollbackPath: String?
    let registrationMatches: Bool?
    let estimatedBytes: UInt64?
    let requiredFreeBytes: UInt64?
    let availableBytes: UInt64?
    let ready: Bool?
    var failure: BrowserHostFailure {
        BrowserHostFailure(message: String((message ?? "PhotoFindry could not finish this action.").prefix(4096)),
            nextAction: String((nextAction ?? "Choose Try again. Keep the current library and its recovery copy in place.").prefix(4096)))
    }
    func checkedStatus() throws -> Self {
        guard ok, let state, ["new", "needs-handover", "ready", "busy", "incompatible"].contains(state) else {
            if !ok { throw failure }
            throw BrowserHostFailure.invalidReply
        }
        return self
    }
}

struct BrowserCommandResult {
    let exitCode: Int32
    let response: Data?
    let diagnostic: String
    func decoded() throws -> BrowserMigrationReply {
        guard let response, response.count <= 65536,
              let reply = try? JSONDecoder().decode(BrowserMigrationReply.self, from: response) else {
            throw BrowserHostFailure.invalidReply
        }
        guard exitCode == 0, reply.ok else { throw reply.failure }
        return reply
    }
}

protocol BrowserCommandRunning: AnyObject {
    func run(executable: URL, arguments: [String], json: Bool,
             progress: @escaping @Sendable (BrowserMigrationProgress) -> Void) async throws -> BrowserCommandResult
    /// Interrupt only this runner's migration control process and await its
    /// cooperative worker/lease drain. Never target a process by name or PID
    /// discovered outside this runner.
    func cancelMigration() async
}

enum BrowserMigrationScreen: Equatable {
    case checking
    case offer(existing: Bool, diskExplanation: String)
    case working(stage: String, message: String)
    case ready(rollbackPath: String?)
    case failed(BrowserHostFailure)
}

/// Only the bundled CLI opens stores, verifies backups, changes admission or
/// grants browser authority. This controller owns presentation and invocation.
@MainActor final class BrowserMigrationController {
    private let runner: any BrowserCommandRunning
    let paths: BrowserHostPaths
    private let rollbackURL: () -> URL
    private(set) var screen: BrowserMigrationScreen = .checking { didSet { changed?(screen) } }
    private(set) var busy = false
    private(set) var canCancelMigration = false
    private(set) var hasOpenedWorkspace = false
    private var configured = false
    private var cancellationRequested = false
    private var pendingRequest: BrowserLaunchRequest = .workspace
    var changed: ((BrowserMigrationScreen) -> Void)?

    init(paths: BrowserHostPaths, runner: any BrowserCommandRunning, rollbackURL: (() -> URL)? = nil) {
        self.paths = paths; self.runner = runner
        self.rollbackURL = rollbackURL ?? { paths.rollbackDirectory.appendingPathComponent("Library-" + UUID().uuidString, isDirectory: true) }
    }

    func start(request: BrowserLaunchRequest = .workspace) async {
        // A public link can request a bounded archive identity, never a library,
        // runtime, executable, callback, credentials or an approval decision.
        pendingRequest = request
        guard !busy else { return }
        busy = true
        cancellationRequested = false
        defer { busy = false }
        do {
            if configured { try await openPendingRequest(); return }
            screen = .checking
            let result = try await command(["migration-status", "--data-dir", paths.library.path])
            let reply = try result.decoded().checkedStatus()
            switch reply.state {
            case "new", "needs-handover":
                var disk = "PhotoFindry will check compatibility and available disk space before making changes."
                if let required = reply.requiredFreeBytes, let available = reply.availableBytes {
                    disk = "Recovery space needed: \(ByteCountFormatter.string(fromByteCount: Int64(clamping: required), countStyle: .file)). Available: \(ByteCountFormatter.string(fromByteCount: Int64(clamping: available), countStyle: .file))."
                }
                screen = .offer(existing: reply.state == "needs-handover", diskExplanation: disk)
            case "ready":
                try await migrateAndOpen()
            case "busy", "incompatible": throw reply.failure
            default: throw BrowserHostFailure.invalidReply
            }
        } catch { screen = .failed(asFailure(error)) }
    }

    func acceptMigration() async {
        guard !busy, case .offer = screen else { return }
        busy = true
        cancellationRequested = false
        defer { busy = false }
        do { try await migrateAndOpen() }
        catch { screen = .failed(asFailure(error)) }
    }

    private func migrateAndOpen() async throws {
        canCancelMigration = true
        defer { canCancelMigration = false }
        screen = .working(stage: "checking", message: "Checking library compatibility and recovery space…")
        let recovery = rollbackURL()
        let result = try await command(paths.migrationArguments(rollback: recovery))
        canCancelMigration = false
        guard !cancellationRequested else { throw cancelledFailure }
        let reply = try result.decoded()
        guard reply.state == "ready", reply.rollbackPath == nil || reply.rollbackPath == recovery.path else {
            throw BrowserHostFailure.invalidReply
        }
        configured = true
        try await openPendingRequest(rollbackPath: reply.rollbackPath)
    }

    func cancelMigration() async {
        guard busy, canCancelMigration, !cancellationRequested else { return }
        cancellationRequested = true; canCancelMigration = false
        screen = .working(stage: "cancelling", message: "Cancelling preparation and waiting for PhotoFindry’s own library worker to close…")
        await runner.cancelMigration()
        // The original operation owns final presentation; it must finish its
        // process/pipe drain before we allow a retry or termination.
    }

    private var cancelledFailure: BrowserHostFailure {
        .init(message: "Browser preparation was cancelled. Your library and any recovery copy remain in place.",
            nextAction: "Choose Try again when you are ready to continue, or Quit for now. PhotoFindry will recheck the saved state before continuing.")
    }

    private func openPendingRequest(rollbackPath: String? = nil) async throws {
        screen = .working(stage: "opening", message: "Opening your trusted local workspace in the browser…")
        let request = pendingRequest
        let result = try await runner.run(executable: paths.executable, arguments: request.arguments + ["--launch-state", paths.launchState.path], json: false, progress: { _ in })
        guard result.exitCode == 0 else {
            throw BrowserHostFailure(message: "Your library is ready, but its browser workspace could not open.",
                nextAction: result.diagnostic.isEmpty ? "Choose Try again to reopen PhotoFindry. Your saved library and recovery copy remain in place." : result.diagnostic)
        }
        hasOpenedWorkspace = true
        screen = .ready(rollbackPath: rollbackPath)
        if pendingRequest != request { try await openPendingRequest(rollbackPath: rollbackPath) }
    }

    func prepareForExit() async throws {
        guard !busy else {
            throw BrowserHostFailure(message: "PhotoFindry is still preparing your library.",
                nextAction: "Wait for preparation to finish. Your recovery copy and library must finish closing before PhotoFindry can quit or install an update.")
        }
        // Declining the offer must not stop a pre-migration native app or
        // require a library to exist merely to close this setup window.
        // The installed background service owns its lifetime after handoff.
        // Closing the one-time installer must not stop it.
        return
    }

    /// Installation must prove package idleness even before the user accepts
    /// library handover: another registered library may use this same payload.
    /// Ordinary Quit for now deliberately has different, non-adopting behavior.
    func prepareForUpdate() async throws {
        guard !busy else {
            throw BrowserHostFailure(message: "PhotoFindry is still preparing your library.",
                nextAction: "Wait for preparation to finish or cancel it, then choose Install Downloaded Update again.")
        }
        busy = true
        defer { busy = false }
        let result = try await command(["prepare-app-update", "--data-dir", paths.library.path])
        let reply = try result.decoded()
        guard reply.ready == true else { throw reply.failure }
    }

    private func command(_ arguments: [String]) async throws -> BrowserCommandResult {
        try await runner.run(executable: paths.executable, arguments: arguments, json: true, progress: { [weak self] event in
            Task { @MainActor in
                guard let self, self.busy, !self.cancellationRequested, case .working = self.screen else { return }
                self.screen = .working(stage: event.stage, message: event.message)
            }
        })
    }

    private func asFailure(_ error: Error) -> BrowserHostFailure {
        if cancellationRequested { return cancelledFailure }
        return error as? BrowserHostFailure ?? BrowserHostFailure(message: "PhotoFindry could not finish preparing the browser workspace.",
            nextAction: "Choose Try again. If it still fails, quit the other PhotoFindry instance and reopen this app. Keep the library and its recovery copy in place.")
    }
}
