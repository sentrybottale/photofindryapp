import Foundation
import Darwin

/// Drains both pipes continuously; retains one bounded final JSON frame and a
/// small diagnostic only. No bootstrap capability is captured or logged.
final class BrowserProcessOutput: @unchecked Sendable {
    private let lock = NSLock()
    private var pending = Data()
    private var final: Data?
    private var errors = Data()
    private var invalid = false
    private let json: Bool
    private let progress: @Sendable (BrowserMigrationProgress) -> Void
    init(json: Bool, progress: @escaping @Sendable (BrowserMigrationProgress) -> Void) {
        self.json = json; self.progress = progress
    }
    func stdout(_ data: Data, eof: Bool = false) {
        guard json else { return }
        lock.lock()
        defer { lock.unlock() }
        guard !invalid else { return }
        for byte in data {
            if byte == 10 { consumeLine(); pending.removeAll(keepingCapacity: true) }
            else if pending.count < 65536 { pending.append(byte) }
            else { invalid = true; pending.removeAll(); final = nil; return }
        }
        if eof, !pending.isEmpty { consumeLine(); pending.removeAll() }
    }
    private func consumeLine() {
        guard !pending.isEmpty else { return }
        guard let object = try? JSONSerialization.jsonObject(with: pending) as? [String: Any] else {
            invalid = true; final = nil; return
        }
        if object["event"] != nil {
            guard final == nil, let event = try? JSONDecoder().decode(BrowserMigrationProgress.self, from: pending), event.isValid else {
                invalid = true; final = nil; return
            }
            progress(event)
        } else if final == nil { final = pending }
        else { invalid = true; final = nil }
    }
    func stderr(_ data: Data) {
        lock.lock(); defer { lock.unlock() }
        errors.append(data.prefix(max(0, 4096 - errors.count)))
    }
    func result(exitCode: Int32) -> BrowserCommandResult {
        lock.lock(); defer { lock.unlock() }
        return BrowserCommandResult(exitCode: exitCode, response: invalid ? nil : final,
            diagnostic: String(decoding: errors, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines))
    }
}

final class BrowserProcessRunner: BrowserCommandRunning, @unchecked Sendable {
    private let lock = NSLock()
    private var active: Process?
    private var activeCommand: String?
    private var cancellationRequested = false
    private let timeout: @Sendable (String) -> TimeInterval?
    private let interruptGrace: TimeInterval
    private let pipeDrainGrace: TimeInterval

    init(timeout: @escaping @Sendable (String) -> TimeInterval? = { command in
        switch command {
        case "migrate-library", "lifecycle": return nil // Cooperative, explicitly cancellable cold-copy transaction.
        case "migration-status": return 15
        case "prepare-app-update": return 90 // Includes the CLI's bounded 60-second owned drain.
        default: return 30
        }
    }, interruptGrace: TimeInterval = 5, pipeDrainGrace: TimeInterval = 2) {
        self.timeout = timeout; self.interruptGrace = interruptGrace; self.pipeDrainGrace = pipeDrainGrace
    }

    func run(executable: URL, arguments: [String], json: Bool,
             progress: @escaping @Sendable (BrowserMigrationProgress) -> Void) async throws -> BrowserCommandResult {
        let process = Process()
        let command = arguments.first ?? ""
        let admitted = lock.withLock {
            guard active == nil else { return false }
            active = process; activeCommand = command; cancellationRequested = false
            return true
        }
        guard admitted else { throw BrowserHostFailure.invalidReply }
        defer { lock.withLock { active = nil; activeCommand = nil } }
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                process.executableURL = executable; process.arguments = arguments
                process.standardInput = FileHandle.nullDevice
                // Ignore operational developer overrides when a signed launcher
                // was started from a browser or Finder. Paths are fixed argv.
                process.environment = ProcessInfo.processInfo.environment.filter { key, _ in
                    !key.hasPrefix("PHOTORECALL_") && !key.hasPrefix("PHOTOFINDRY_") && !key.hasPrefix("DYLD_")
                }
                let output = Pipe(), errors = Pipe()
                process.standardOutput = output; process.standardError = errors
                var descriptors = [output.fileHandleForReading.fileDescriptor, errors.fileHandleForReading.fileDescriptor].map {
                    pollfd(fd: $0, events: Int16(POLLIN | POLLHUP | POLLERR), revents: 0)
                }
                for descriptor in descriptors {
                    let flags = fcntl(descriptor.fd, F_GETFL)
                    guard flags >= 0, fcntl(descriptor.fd, F_SETFL, flags | O_NONBLOCK) == 0 else {
                        continuation.resume(throwing: BrowserHostFailure.invalidReply); return
                    }
                }
                let captured = BrowserProcessOutput(json: json, progress: progress)
                self.lock.lock(); let cancelledBeforeLaunch = self.cancellationRequested; self.lock.unlock()
                if cancelledBeforeLaunch {
                    continuation.resume(throwing: CancellationError()); return
                }
                do { try process.run() }
                catch { continuation.resume(throwing: error); return }
                self.lock.lock(); let cancelledAfterLaunch = self.cancellationRequested; self.lock.unlock()
                if cancelledAfterLaunch, process.isRunning { process.interrupt() }
                let deadline = self.timeout(command).map { ContinuousClock.now.advanced(by: .seconds($0)) }
                // Poll nonblocking owned descriptors on this same thread.
                // A descendant can inherit a writer after our direct child
                // exits; EOF has its own bound and never requires killing that
                // unrelated descendant or closing another thread's read call.
                var eof = [false, false]
                var pipeDeadline: ContinuousClock.Instant?
                var pipeTimedOut = false
                var ioFailed = false
                var timedOut = false
                var signalDeadline: ContinuousClock.Instant?
                var sentTerminate = false
                var sentKill = false
                let capacity = 8192
                var buffer = [UInt8](repeating: 0, count: capacity)
                while process.isRunning || !eof.allSatisfy({ $0 }) {
                    let now = ContinuousClock.now
                    if !process.isRunning {
                        if pipeDeadline == nil { pipeDeadline = now.advanced(by: .seconds(self.pipeDrainGrace)) }
                        if let pipeDeadline, now >= pipeDeadline { pipeTimedOut = true; break }
                    } else if !timedOut, let deadline, now >= deadline {
                        timedOut = true; process.interrupt()
                        signalDeadline = now.advanced(by: .seconds(self.interruptGrace))
                    } else if timedOut, let due = signalDeadline, now >= due, !sentKill {
                        if !sentTerminate {
                            process.terminate(); sentTerminate = true
                            signalDeadline = now.advanced(by: .seconds(self.interruptGrace))
                        } else {
                            // This path is limited to the retained one-shot
                            // control child; migrate-library has no timer.
                            _ = Darwin.kill(process.processIdentifier, SIGKILL); sentKill = true
                        }
                    }
                    let polled = Darwin.poll(&descriptors, nfds_t(descriptors.count), 50)
                    if polled < 0, errno != EINTR { ioFailed = true }
                    for index in descriptors.indices where !eof[index] {
                        guard descriptors[index].revents != 0 else { continue }
                        for _ in 0..<8 {
                            let received = buffer.withUnsafeMutableBytes { Darwin.read(descriptors[index].fd, $0.baseAddress, capacity) }
                            if received > 0 {
                                let data = Data(buffer.prefix(received))
                                if index == 0 { captured.stdout(data) } else { captured.stderr(data) }
                            } else if received == 0 {
                                eof[index] = true; descriptors[index].fd = -1
                                if index == 0 { captured.stdout(Data(), eof: true) }
                                break
                            } else if errno == EAGAIN || errno == EWOULDBLOCK { break }
                            else if errno == EINTR { continue }
                            else { ioFailed = true; eof[index] = true; descriptors[index].fd = -1; break }
                        }
                    }
                }
                process.waitUntilExit()
                try? output.fileHandleForReading.close(); try? errors.fileHandleForReading.close()
                if timedOut {
                    continuation.resume(throwing: BrowserHostFailure(message: "PhotoFindry’s local control request took too long.",
                        nextAction: "Choose Try again. If the local workspace is already open, check Jobs and pause active work before retrying. No unrelated process was stopped."))
                    return
                }
                if pipeTimedOut || ioFailed {
                    continuation.resume(throwing: BrowserHostFailure(message: "PhotoFindry’s local companion did not close its response stream cleanly.",
                        nextAction: "Choose Try again. The completed control process has stopped; saved work and unrelated processes have been kept."))
                    return
                }
                continuation.resume(returning: captured.result(exitCode: process.terminationStatus))
            }
        }
    }

    func cancelMigration() async {
        let process: Process? = lock.withLock {
            guard ["migrate-library", "lifecycle"].contains(activeCommand ?? ""), let process = active else { return nil }
            cancellationRequested = true
            return process
        }
        guard let process else { return }
        if process.isRunning { process.interrupt() }
        // A migration may be making a consistent backup or closing its own
        // worker. Its CLI handles SIGINT and owns the drain; never force kill
        // it or an independently running model to make Cancel appear instant.
        while true {
            let stillOwned = lock.withLock { active === process }
            if !stillOwned { return }
            try? await Task.sleep(nanoseconds: 50_000_000)
        }
    }
}
