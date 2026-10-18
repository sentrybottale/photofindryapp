import Foundation
import XCTest
@testable import PhotoFindryBrowserHost

final class BrowserProcessTests: XCTestCase {
    func testChunkedProgressAndFinalResultPreserveOrderingAndBoundedDiagnostic() throws {
        let events = CollectedProgress()
        let output = BrowserProcessOutput(json: true, progress: { events.append($0) })
        let text = #"{"event":"progress","stage":"backup","message":"Verifying recovery copy…"}"# + "\n" + #"{"ok":true,"state":"ready"}"# + "\n"
        for byte in text.utf8 { output.stdout(Data([byte])) }
        output.stdout(Data(), eof: true)
        output.stderr(Data(repeating: 65, count: 20000))
        let result = output.result(exitCode: 0)
        XCTAssertEqual(events.snapshot.map(\.stage), ["backup"])
        XCTAssertEqual(try result.decoded().state, "ready")
        XCTAssertEqual(result.diagnostic.utf8.count, 4096)
    }

    func testOversizedMalformedDuplicateOrPostFinalProgressCannotBecomeReadiness() {
        let invalid = [String(repeating: "a", count: 65537), "no JSON\n",
            #"{"ok":true,"state":"ready"}"# + "\n" + #"{"ok":true,"state":"ready"}"# + "\n",
            #"{"ok":true,"state":"ready"}"# + "\n" + #"{"event":"progress","stage":"backup","message":"late"}"# + "\n",
            #"{"event":"progress","stage":"delete-originals","message":"unsafe"}"# + "\n"]
        for text in invalid {
            let output = BrowserProcessOutput(json: true, progress: { _ in })
            output.stdout(Data(text.utf8)); output.stdout(Data(), eof: true)
            XCTAssertThrowsError(try output.result(exitCode: 0).decoded())
        }
    }

    func testNonJSONOpenOutputIsNotRetained() {
        let output = BrowserProcessOutput(json: false, progress: { _ in XCTFail("No progress from opener") })
        output.stdout(Data("a capability must not be captured".utf8), eof: true)
        XCTAssertNil(output.result(exitCode: 0).response)
    }

    func testRealOwnedSubprocessDrainsAndDeliversItsFinalJSONWithoutShell() async throws {
        let runner = BrowserProcessRunner()
        let result = try await runner.run(executable: URL(fileURLWithPath: "/usr/bin/printf"),
            arguments: ["%s\n", #"{"ok":true,"state":"ready"}"#], json: true, progress: { _ in })
        XCTAssertEqual(try result.decoded().state, "ready")
    }

    func testOwnedReadOnlyControlTimeoutIsBoundedAndActionable() async {
        let runner = BrowserProcessRunner(timeout: { _ in 0.05 }, interruptGrace: 0.1)
        let started = ContinuousClock.now
        do {
            _ = try await runner.run(executable: URL(fileURLWithPath: "/bin/sleep"),
                arguments: ["10"], json: false, progress: { _ in })
            XCTFail("Timeout must be explicit")
        } catch { XCTAssertTrue(error.localizedDescription.contains("Try again")) }
        XCTAssertLessThan(started.duration(to: .now), .seconds(2))
    }

    func testMigrationCancelInterruptsOnlyItsOwnedControlAndAwaitsExit() async throws {
        let runner = BrowserProcessRunner()
        let work = Task {
            try await runner.run(executable: URL(fileURLWithPath: "/usr/bin/yes"),
                arguments: ["migrate-library"], json: false, progress: { _ in })
        }
        try await Task.sleep(nanoseconds: 100_000_000)
        await runner.cancelMigration()
        let result = try await work.value
        XCTAssertNotEqual(result.exitCode, 0)
        // A subsequent request proves the old child and runner ownership drained.
        let next = try await runner.run(executable: URL(fileURLWithPath: "/usr/bin/printf"),
            arguments: ["%s\n", #"{"ok":true,"state":"ready"}"#], json: true, progress: { _ in })
        XCTAssertEqual(try next.decoded().state, "ready")
    }

    func testInheritedDescendantWriterCannotHoldOwnedControlCompletionForever() async throws {
        let runner = BrowserProcessRunner(timeout: { _ in 2 }, interruptGrace: 0.1, pipeDrainGrace: 0.1)
        let started = ContinuousClock.now
        do {
            // Only this synthetic child is started by the test. Its short-lived
            // background sleep deliberately inherits stdout/stderr. The runner
            // must close its own descriptors, never signal that descendant.
            _ = try await runner.run(executable: URL(fileURLWithPath: "/bin/sh"),
                arguments: ["-c", "sleep 1 & printf '%s\\n' '{\"ok\":true,\"state\":\"ready\"}'"],
                json: true, progress: { _ in })
            XCTFail("An inherited pipe must be an explicit bounded failure")
        } catch { XCTAssertTrue(error.localizedDescription.contains("response stream")) }
        XCTAssertLessThan(started.duration(to: .now), .milliseconds(750))
        let next = try await runner.run(executable: URL(fileURLWithPath: "/usr/bin/printf"),
            arguments: ["%s\n", #"{"ok":true,"state":"ready"}"#], json: true, progress: { _ in })
        XCTAssertEqual(try next.decoded().state, "ready")
        // Allow the harmless test descendant to finish naturally.
        try await Task.sleep(nanoseconds: 1_000_000_000)
    }
}

private final class CollectedProgress: @unchecked Sendable {
    private let lock = NSLock()
    private var events = [BrowserMigrationProgress]()
    func append(_ event: BrowserMigrationProgress) { lock.lock(); defer { lock.unlock() }; events.append(event) }
    var snapshot: [BrowserMigrationProgress] { lock.lock(); defer { lock.unlock() }; return events }
}
