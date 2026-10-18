import AppKit
import CoreServices

protocol SourceWatchingStream: AnyObject {
    /// Runs only on the lifecycle engine's serial background queue.
    func stop()
}

private final class SourceEventSink {
    let receive: @Sendable ([String], Bool) -> Void
    init(receive: @escaping @Sendable ([String], Bool) -> Void) { self.receive = receive }
}

private final class NativeSourceWatchingStream: SourceWatchingStream {
    private var stream: FSEventStreamRef?
    private static let eventQueue = DispatchQueue(label: "PhotoFindry.source-events", qos: .utility)
    private init(_ stream: FSEventStreamRef) { self.stream = stream }

    /// FSEventStreamCreate itself opens watched paths. A stalled file provider
    /// can block here before a stream exists, so even creation stays off the UI.
    static func create(_ roots: [String], receive: @escaping @Sendable ([String], Bool) -> Void) -> SourceWatchingStream? {
        let pointer = Unmanaged.passRetained(SourceEventSink(receive: receive)).toOpaque()
        var context = FSEventStreamContext(version: 0, info: pointer, retain: nil,
            release: { info in if let info { Unmanaged<SourceEventSink>.fromOpaque(info).release() } }, copyDescription: nil)
        let flags = FSEventStreamCreateFlags(kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagWatchRoot | kFSEventStreamCreateFlagIgnoreSelf)
        guard let created = FSEventStreamCreate(nil, { _, info, count, paths, flags, _ in
            guard let info else { return }
            let sink = Unmanaged<SourceEventSink>.fromOpaque(info).takeUnretainedValue()
            let values = unsafeBitCast(paths, to: NSArray.self)
            var collected: [String] = []
            var dropped = count > 256
            let reconciliationFlags = FSEventStreamEventFlags(kFSEventStreamEventFlagMustScanSubDirs | kFSEventStreamEventFlagUserDropped | kFSEventStreamEventFlagKernelDropped | kFSEventStreamEventFlagEventIdsWrapped | kFSEventStreamEventFlagRootChanged | kFSEventStreamEventFlagMount | kFSEventStreamEventFlagUnmount)
            for i in 0..<min(count, 256) {
                if flags[i] & reconciliationFlags != 0 { dropped = true }
                if let path = values[i] as? String { collected.append(path) }
            }
            sink.receive(collected, dropped)
        }, &context, roots as CFArray, FSEventStreamEventId(kFSEventStreamEventIdSinceNow), 2, flags) else {
            Unmanaged<SourceEventSink>.fromOpaque(pointer).release(); return nil
        }
        FSEventStreamSetDispatchQueue(created, eventQueue)
        guard FSEventStreamStart(created) else {
            FSEventStreamInvalidate(created); FSEventStreamRelease(created); return nil
        }
        return NativeSourceWatchingStream(created)
    }

    func stop() {
        guard let stream else { return }
        FSEventStreamStop(stream); FSEventStreamInvalidate(stream); FSEventStreamRelease(stream)
        self.stream = nil
    }
}

/// One blocking lifecycle operation and one replaceable desired configuration.
/// No filesystem call is made while holding the admission lock. A provider that
/// never returns cannot block MainActor or accumulate queued rebuild closures.
final class SourceStreamLifecycle: @unchecked Sendable {
    typealias Factory = @Sendable ([String], @escaping @Sendable ([String], Bool) -> Void) -> SourceWatchingStream?
    private struct Desired {
        var generation: UInt64 = 0
        var roots: [String] = []
        var active = false
    }
    private let factory: Factory
    private let receive: @MainActor @Sendable ([String], Bool) -> Void
    private let queue = DispatchQueue(label: "PhotoFindry.source-watch-lifecycle", qos: .utility)
    private let lock = NSLock()
    private var desired = Desired()
    private var scheduled = false
    private var stream: SourceWatchingStream? // accessed only on queue
    private var eventPaths = Set<String>()
    private var eventHistoryDropped = false
    private var eventDeliveryScheduled = false

    init(factory: @escaping Factory, receive: @escaping @MainActor @Sendable ([String], Bool) -> Void) {
        self.factory = factory; self.receive = receive
    }

    func update(roots: [String], active: Bool, force: Bool = false) {
        let bounded = Array(roots.prefix(256))
        lock.lock()
        guard force || desired.active != active || desired.roots != bounded else { lock.unlock(); return }
        desired = Desired(generation: desired.generation &+ 1, roots: bounded, active: active)
        eventPaths.removeAll(); eventHistoryDropped = false
        let start = !scheduled
        scheduled = true
        lock.unlock()
        if start { queue.async { self.pump() } }
    }

    private func isCurrent(_ generation: UInt64) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return desired.generation == generation
    }

    private func pump() {
        while true {
            lock.lock(); let request = desired; lock.unlock()
            stream?.stop(); stream = nil
            // Stop/invalidate can block too. Do not create an obsolete watcher
            // when newer roots (or Stop) arrived while the old stream stopped.
            if !isCurrent(request.generation) { continue }
            if request.active && !request.roots.isEmpty {
                let generation = request.generation
                let created = factory(request.roots) { [weak self] paths, dropped in
                    self?.enqueueEvents(paths, historyDropped: dropped, generation: generation)
                }
                if isCurrent(generation) { stream = created }
                else { created?.stop() }
            }
            lock.lock()
            if desired.generation == request.generation {
                scheduled = false; lock.unlock(); return
            }
            lock.unlock()
        }
    }

    private func enqueueEvents(_ paths: [String], historyDropped: Bool, generation: UInt64) {
        lock.lock()
        guard desired.active, desired.generation == generation else { lock.unlock(); return }
        eventHistoryDropped = eventHistoryDropped || historyDropped || paths.count > 256
        if !eventHistoryDropped {
            for path in paths.prefix(256) where path.utf8.count <= 16_384 {
                eventPaths.insert(path)
                if eventPaths.count > 256 { eventHistoryDropped = true; break }
            }
        }
        if eventHistoryDropped { eventPaths.removeAll() }
        let start = !eventDeliveryScheduled; eventDeliveryScheduled = true
        lock.unlock()
        if start { Task { @MainActor in self.deliverEvents() } }
    }

    @MainActor private func deliverEvents() {
        lock.lock()
        let paths = eventPaths.sorted(), dropped = eventHistoryDropped, active = desired.active
        eventPaths.removeAll(); eventHistoryDropped = false; eventDeliveryScheduled = false
        lock.unlock()
        if active && (!paths.isEmpty || dropped) { receive(paths, dropped) }
    }
}

/// Change notifications are hints for bounded reconciliation, never evidence
/// that a photo was removed. Watches only registered roots and reads no pixels.
@MainActor
final class SourceVolumeMonitor {
    private let onChange: ([String]) -> Void
    private let debounceInterval: TimeInterval
    private var roots: [String] = []
    private var observers: [NSObjectProtocol] = []
    private var lifecycle: SourceStreamLifecycle!
    private var delivery: DispatchWorkItem?
    private var pending = Set<String>()
    private var started = false

    init(debounceInterval: TimeInterval = 2,
         streamFactory: @escaping SourceStreamLifecycle.Factory = { NativeSourceWatchingStream.create($0, receive: $1) },
         onChange: @escaping ([String]) -> Void) {
        self.onChange = onChange; self.debounceInterval = max(0, debounceInterval)
        lifecycle = SourceStreamLifecycle(factory: streamFactory) { [weak self] paths, dropped in
            guard let self else { return }
            self.enqueue(Self.affectedRoots(self.roots, paths: paths, historyDropped: dropped))
        }
    }

    deinit { lifecycle?.update(roots: [], active: false) }

    func updateRoots(_ paths: [String]) {
        // Lexical normalization only. URL resource/symlink resolution must not
        // contact a file provider from this main-actor snapshot handler.
        let next = Array(Set(paths.compactMap(Self.normalizedRoot))).sorted()
        guard next != roots else { return }
        roots = next
        if started { lifecycle.update(roots: roots, active: true) }
    }

    static func normalizedRoot(_ path: String) -> String? {
        guard path.hasPrefix("/"), !path.contains("\0"), path.utf8.count <= 16_384 else { return nil }
        var parts: [Substring] = []
        for part in path.split(separator: "/") {
            if part == "." { continue }
            if part == ".." { if !parts.isEmpty { parts.removeLast() } }
            else { parts.append(part) }
        }
        return "/" + parts.joined(separator: "/")
    }

    func start() {
        guard !started else { return }
        started = true
        let center = NSWorkspace.shared.notificationCenter
        for name in [NSWorkspace.didMountNotification, NSWorkspace.didUnmountNotification, NSWorkspace.didRenameVolumeNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, self.started else { return }
                    // Retry a previously unavailable watch after a mount hint.
                    self.lifecycle.update(roots: self.roots, active: true, force: true)
                    self.enqueue(self.roots)
                }
            })
        }
        lifecycle.update(roots: roots, active: true)
    }

    func stop() {
        started = false
        delivery?.cancel(); delivery = nil; pending.removeAll()
        for observer in observers { NSWorkspace.shared.notificationCenter.removeObserver(observer) }
        observers.removeAll()
        lifecycle.update(roots: [], active: false)
    }

    /// Convert all file and dropped-history events into registered-root hints.
    /// Cursors are intentionally not persisted by this lightweight observer;
    /// durable, restart-safe inventory checkpoints belong to the jobs provider.
    static func affectedRoots(_ roots: [String], paths: [String], historyDropped: Bool) -> [String] {
        if historyDropped { return roots }
        return roots.filter { root in paths.contains { path in
            path == root || path.hasPrefix(root == "/" ? "/" : root + "/") || root.hasPrefix(path == "/" ? "/" : path + "/")
        } }
    }

    private func enqueue(_ paths: [String]) {
        guard started else { return }
        pending.formUnion(paths)
        // Keep one scheduled delivery: continuous writes cannot postpone checks
        // forever and cannot create an unbounded task backlog.
        guard delivery == nil else { return }
        let item = DispatchWorkItem { [weak self] in
            guard let self, self.started else { return }
            self.delivery = nil
            let paths = self.pending.sorted(); self.pending.removeAll()
            if !paths.isEmpty { self.onChange(paths) }
        }
        delivery = item
        DispatchQueue.main.asyncAfter(deadline: .now() + debounceInterval, execute: item)
    }
}
