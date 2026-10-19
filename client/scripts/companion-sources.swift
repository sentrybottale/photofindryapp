import AppKit
import Foundation
import Darwin

// Build together with the unchanged SourceVolumeMonitor.swift. The helper observes
// attributes/FSEvents only; its requests cannot read, decode or modify photographs.
private let maximumFrame = 4 * 1024 * 1024 + 65_536

private final class RootOutput: @unchecked Sendable {
    private let lock = NSLock()
    private let queue = DispatchQueue(label: "PhotoFindry.companion-source-output", qos: .utility)
    private var pending = Set<String>()
    private var generation: UInt64 = 0
    private var writing = false
    private var closed = false

    func emit(roots: [String], generation: UInt64) {
        lock.lock()
        guard !closed else { lock.unlock(); return }
        if self.generation != generation { self.generation = generation; pending.removeAll() }
        for root in roots.prefix(256) where root.utf8.count <= 16_384 { pending.insert(root) }
        // At most 256 registered roots exist in one configuration.
        if pending.count > 256 { pending = Set(pending.sorted().prefix(256)) }
        let start = !writing; writing = true
        lock.unlock()
        if start { queue.async { self.flush() } }
    }

    private func flush() {
        while true {
            lock.lock()
            guard !closed, !pending.isEmpty else { writing = false; lock.unlock(); return }
            let roots = pending.sorted(), identity = generation
            pending.removeAll(); lock.unlock()
            do {
                var bytes = try JSONSerialization.data(withJSONObject: ["generation": identity, "roots": roots])
                guard bytes.count <= maximumFrame else { throw CocoaError(.fileWriteOutOfSpace) }
                bytes.append(10)
                try FileHandle.standardOutput.write(contentsOf: bytes)
            } catch {
                // Do not log to a failed pipe. Exit only this owned observer.
                lock.lock(); closed = true; pending.removeAll(); lock.unlock()
                Darwin.exit(1)
            }
        }
    }
}

private struct RootConfiguration: Sendable {
    var generation: UInt64
    var roots: [String]
}

/// The stdin reader decodes bounded frames off the main queue. A single pending
/// main-actor delivery carries only the latest root configuration.
private final class RootInput: @unchecked Sendable {
    private let lock = NSLock()
    private var latest: RootConfiguration?
    private var deliveryScheduled = false
    private let receive: @MainActor @Sendable (RootConfiguration) -> Void
    private let finish: @MainActor @Sendable () -> Void

    init(receive: @escaping @MainActor @Sendable (RootConfiguration) -> Void,
         finish: @escaping @MainActor @Sendable () -> Void) {
        self.receive = receive; self.finish = finish
    }

    func start() {
        DispatchQueue(label: "PhotoFindry.companion-source-input", qos: .utility).async { self.read() }
    }

    private func publish(_ configuration: RootConfiguration) {
        lock.lock()
        latest = configuration
        let start = !deliveryScheduled; deliveryScheduled = true
        lock.unlock()
        if start { Task { @MainActor in self.deliver() } }
    }

    @MainActor private func deliver() {
        lock.lock(); let next = latest; latest = nil; deliveryScheduled = false; lock.unlock()
        if let next { receive(next) }
    }

    private func read() {
        var frame = Data()
        var malformed = false
        let descriptor = FileHandle.standardInput.fileDescriptor
        var buffer = [UInt8](repeating: 0, count: 8192)
        while !malformed {
            let amount = Darwin.read(descriptor, &buffer, buffer.count)
            if amount < 0 && errno == EINTR { continue }
            if amount <= 0 { break }
            for byte in buffer.prefix(amount) {
                if byte == 10 {
                    guard let configuration = Self.decode(frame) else { malformed = true; break }
                    frame.removeAll(keepingCapacity: true)
                    publish(configuration)
                } else {
                    guard frame.count < maximumFrame else { malformed = true; break }
                    frame.append(byte)
                }
            }
        }
        Task { @MainActor in self.finish() }
    }

    private static func decode(_ bytes: Data) -> RootConfiguration? {
        guard let object = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
              Set(object.keys) == ["generation", "roots"],
              let generation = object["generation"] as? NSNumber,
              CFGetTypeID(generation) != CFBooleanGetTypeID(),
              generation.doubleValue >= 0, generation.doubleValue <= 9_007_199_254_740_991,
              generation.doubleValue.rounded(.down) == generation.doubleValue,
              let roots = object["roots"] as? [String], roots.count <= 256,
              roots.allSatisfy({ $0.hasPrefix("/") && !$0.contains("\0") && $0.utf8.count <= 16_384 }) else { return nil }
        return RootConfiguration(generation: generation.uint64Value, roots: roots)
    }
}

@main private struct CompanionSourcesMain {
    @MainActor static func main() {
        signal(SIGPIPE, SIG_IGN)
        let output = RootOutput()
        var generation: UInt64 = 0
        let monitor = SourceVolumeMonitor { roots in output.emit(roots: roots, generation: generation) }
        let input = RootInput(receive: { configuration in
            generation = configuration.generation
            monitor.updateRoots(configuration.roots)
            // A file may return before the asynchronous watch stream exists.
            // Reconcile each new registered-root configuration once so that
            // this startup gap cannot leave a saved job waiting indefinitely.
            // This is only a hint; the worker still verifies locations/identity
            // and preserves queue, source and resource holds.
            output.emit(roots: configuration.roots, generation: generation)
        }, finish: {
            monitor.stop()
            Darwin.exit(0)
        })
        // NSWorkspace volume notifications need the main run loop, but no window,
        // Dock presence or GUI application launch is needed for this observer.
        monitor.start(); input.start()
        withExtendedLifetime((monitor, input, output)) { RunLoop.main.run() }
    }
}
