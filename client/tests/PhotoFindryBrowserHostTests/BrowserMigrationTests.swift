import Foundation
import XCTest
@testable import PhotoFindryBrowserHost

@MainActor final class BrowserMigrationTests: XCTestCase {
    private let paths = BrowserHostPaths(resources: URL(fileURLWithPath: "/Applications/PhotoFindry.app/Contents/Resources"),
        home: URL(fileURLWithPath: "/Users/some one"))

    func testExistingLibraryWaitsForConsentThenBacksUpAndOpensWithoutImportOrModelArguments() async throws {
        let recovery = paths.rollbackDirectory.appendingPathComponent("frozen-copy")
        let runner = ScriptedBrowserRunner([
            .json(#"{"ok":true,"state":"needs-handover","requiredFreeBytes":2000,"availableBytes":9000}"#),
            .json("{\"ok\":true,\"state\":\"ready\",\"rollbackPath\":\"\(recovery.path)\"}"), .success
        ])
        let controller = BrowserMigrationController(paths: paths, runner: runner, rollbackURL: { recovery })
        await controller.start()
        XCTAssertEqual(runner.invocations.count, 1)
        guard case .offer(let existing, let disk) = controller.screen else { return XCTFail("Offer required") }
        XCTAssertTrue(existing); XCTAssertTrue(disk.contains("Recovery space"))
        await controller.acceptMigration()
        XCTAssertEqual(controller.screen, .ready(rollbackPath: recovery.path))
        XCTAssertEqual(runner.invocations.map(\.arguments), [
            ["migration-status", "--current-library", "--data-dir", paths.library.path], paths.migrationArguments(rollback: recovery), ["open", "--launch-state", paths.launchState.path]
        ])
        XCTAssertTrue(runner.invocations.allSatisfy { $0.executable == paths.executable })
        XCTAssertTrue(controller.hasOpenedWorkspace)
        XCTAssertFalse(runner.invocations.flatMap(\.arguments).contains("--model"))
    }

    func testQuitForNowDoesNotAdmitLibraryOrDrainAnotherNativeApp() async throws {
        let runner = ScriptedBrowserRunner([.json(#"{"ok":true,"state":"needs-handover"}"#)])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start(); try await controller.prepareForExit()
        XCTAssertEqual(runner.invocations.count, 1)
        XCTAssertFalse(controller.hasOpenedWorkspace)
    }

    func testUpdateBeforeConsentChecksOwnedPackageWhileQuitForNowRemainsNonAdopting() async throws {
        let runner = ScriptedBrowserRunner([
            .json(#"{"ok":true,"state":"needs-handover","estimatedBytes":null,"requiredFreeBytes":null,"estimatePending":true,"availableBytes":9000}"#),
            .json(#"{"ok":false,"ready":false,"message":"This app package has an active local companion.","nextAction":"Pause its work and quit it, then retry the update."}"#, code: 1),
            .json(#"{"ok":true,"ready":true}"#)
        ])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start()
        guard case .offer(let existing, let explanation) = controller.screen else { return XCTFail("Null estimate must remain an offer") }
        XCTAssertTrue(existing); XCTAssertTrue(explanation.contains("check compatibility"))
        try await controller.prepareForExit()
        XCTAssertEqual(runner.invocations.count, 1, "Declining must not stop the old native host")
        do { try await controller.prepareForUpdate(); XCTFail("Active package must defer installation") }
        catch { XCTAssertTrue(error.localizedDescription.contains("Pause its work")) }
        try await controller.prepareForUpdate()
        XCTAssertEqual(runner.invocations.suffix(2).map(\.arguments), Array(repeating: ["prepare-app-update", "--current-library", "--data-dir", paths.library.path], count: 2))
        XCTAssertFalse(controller.hasOpenedWorkspace)
        XCTAssertFalse(runner.invocations.contains(where: { $0.arguments.first == "migrate-library" }))
    }

    func testNewLibraryUsesSameProtectedCreateCommandOnlyAfterExplicitAcceptance() async {
        let runner = ScriptedBrowserRunner([.json(#"{"ok":true,"state":"new"}"#), .json(#"{"ok":true,"state":"ready"}"#), .success])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start()
        guard case .offer(let existing, _) = controller.screen else { return XCTFail("Offer required") }
        XCTAssertFalse(existing); XCTAssertEqual(runner.invocations.count, 1)
        await controller.acceptMigration()
        XCTAssertTrue(controller.hasOpenedWorkspace)
    }

    func testReadyMatchingRegistrationStillInstallsIndependentCLIOnce() async {
        let runner = ScriptedBrowserRunner([.json(#"{"ok":true,"state":"ready","registrationMatches":true}"#), .json(#"{"ok":true,"state":"ready"}"#), .success, .success])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start(); await controller.start()
        XCTAssertEqual(runner.invocations.map { $0.arguments.first! }, ["migration-status", "lifecycle", "open", "open"])
    }

    func testReadyNewBundleRebindsThroughCLIInsteadOfUsingOldExecutable() async {
        let runner = ScriptedBrowserRunner([.json(#"{"ok":true,"state":"ready","registrationMatches":false}"#), .json(#"{"ok":true,"state":"ready"}"#), .success])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start()
        XCTAssertEqual(runner.invocations.map { $0.arguments.first! }, ["migration-status", "lifecycle", "open"])
    }

    func testBusyNativeAppHasActionableRecoveryAndRetryReachesWorkspace() async {
        let runner = ScriptedBrowserRunner([
            .json(#"{"ok":true,"state":"busy","message":"The native app is still using this library.","nextAction":"Finish active work and quit the old app, then choose Try again."}"#),
            .json(#"{"ok":true,"state":"needs-handover"}"#), .json(#"{"ok":true,"state":"ready"}"#), .success
        ])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start()
        guard case .failed(let failure) = controller.screen else { return XCTFail("Busy must be actionable") }
        XCTAssertTrue(failure.nextAction.contains("quit the old app"))
        XCTAssertEqual(runner.invocations.count, 1)
        await controller.start(); await controller.acceptMigration()
        XCTAssertTrue(controller.hasOpenedWorkspace)
    }

    func testMigrationFailureDoesNotOpenAndExplicitRetryUsesFreshRecoveryPath() async {
        let runner = ScriptedBrowserRunner([
            .json(#"{"ok":true,"state":"needs-handover"}"#),
            .json(#"{"ok":false,"state":"blocked","message":"Not enough recovery space.","nextAction":"Free disk space, then choose Try again."}"#, code: 1),
            .json(#"{"ok":true,"state":"needs-handover"}"#), .json(#"{"ok":true,"state":"ready"}"#), .success
        ])
        var serial = 0
        let controller = BrowserMigrationController(paths: paths, runner: runner, rollbackURL: {
            serial += 1; return self.paths.rollbackDirectory.appendingPathComponent("copy-\(serial)")
        })
        await controller.start(); await controller.acceptMigration()
        XCTAssertFalse(controller.hasOpenedWorkspace)
        guard case .failed(let failure) = controller.screen else { return XCTFail("Recovery expected") }
        XCTAssertTrue(failure.nextAction.contains("Free disk space"))
        await controller.start(); await controller.acceptMigration()
        XCTAssertTrue(controller.hasOpenedWorkspace)
        XCTAssertNotEqual(runner.invocations[1].arguments.last, runner.invocations[3].arguments.last)
    }

    func testIncompatibleOrMalformedReplyNeverAdmitsOrOpensLibrary() async {
        for response in [#"{"ok":true,"state":"incompatible","nextAction":"Open the compatible old app to finish recovery."}"#,
                         #"{"ok":true,"state":"silently-migrate"}"#, "not JSON"] {
            let runner = ScriptedBrowserRunner([.json(response)])
            let controller = BrowserMigrationController(paths: paths, runner: runner)
            await controller.start(); await controller.acceptMigration()
            guard case .failed = controller.screen else { return XCTFail("Must fail closed") }
            XCTAssertEqual(runner.invocations.count, 1); XCTAssertFalse(controller.hasOpenedWorkspace)
        }
    }

    func testOpenFailureRetriesOpenOnlyWithoutRepeatingSuccessfulMigration() async {
        let runner = ScriptedBrowserRunner([.json(#"{"ok":true,"state":"ready","registrationMatches":true}"#), .json(#"{"ok":true,"state":"ready"}"#),
            BrowserCommandResult(exitCode: 1, response: nil, diagnostic: "Enable your default browser, then choose Try again."), .success])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start()
        guard case .failed(let error) = controller.screen else { return XCTFail("Failed open expected") }
        XCTAssertTrue(error.nextAction.contains("default browser"))
        await controller.start()
        XCTAssertTrue(controller.hasOpenedWorkspace)
        XCTAssertEqual(runner.invocations.map { $0.arguments.first! }, ["migration-status", "lifecycle", "open", "open"])
    }

    func testClosingInstallerKeepsInstalledBackgroundServiceRunning() async throws {
        let runner = ScriptedBrowserRunner([
            .json(#"{"ok":true,"state":"ready","registrationMatches":true}"#),
            .json(#"{"ok":true,"state":"ready"}"#), .success
        ])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start()
        XCTAssertTrue(controller.hasOpenedWorkspace)
        let count = runner.invocations.count
        try await controller.prepareForExit()
        XCTAssertEqual(runner.invocations.count, count, "Closing the installer never drains the independent CLI")
        XCTAssertEqual(runner.invocations[1].arguments.prefix(2), ["lifecycle", "install"])
        XCTAssertTrue(runner.invocations[1].arguments.contains("--release"))
    }

    func testPublicLinksAllowOnlyFixedWorkspaceAndCanonicalArchiveNavigation() {
        XCTAssertEqual(BrowserLaunchRequest(url: URL(string: "photofindry-local://open")!), .workspace)
        XCTAssertEqual(BrowserLaunchRequest(url: URL(string: "photofindry://archive/v1/AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE")!), .archive("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"))
        for value in ["photofindry-local://open?library=/tmp/other", "photofindry-local://open/", "photofindry-local://open#token=hello",
                      "photofindry://archive/v1/../../evil", "photofindry://archive/v1/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee?command=delete",
                      "https://app.photofindry.com/open", "file:///tmp/library", "photofindry://archive/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"] {
            XCTAssertNil(BrowserLaunchRequest(url: URL(string: value)!), value)
        }
    }

    func testRollbackRevealCannotBeRedirectedToAnArbitraryBackendPath() async {
        let runner = ScriptedBrowserRunner([.json(#"{"ok":true,"state":"needs-handover"}"#), .json(#"{"ok":true,"state":"ready","rollbackPath":"/etc/passwd"}"#)])
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start(); await controller.acceptMigration()
        guard case .failed = controller.screen else { return XCTFail("Unrequested path rejected") }
        XCTAssertFalse(controller.hasOpenedWorkspace)
    }

    func testCancelDrainsOwnedMigrationAndFencesLateSuccessBeforeOpen() async {
        let runner = CancellableBrowserRunner()
        let controller = BrowserMigrationController(paths: paths, runner: runner)
        await controller.start()
        let work = Task { await controller.acceptMigration() }
        for _ in 0..<20 where !runner.isMigrating { await Task.yield() }
        XCTAssertTrue(controller.canCancelMigration)
        await controller.cancelMigration()
        await work.value
        XCTAssertEqual(runner.cancellations, 1)
        XCTAssertFalse(controller.hasOpenedWorkspace)
        XCTAssertFalse(controller.busy)
        XCTAssertFalse(controller.canCancelMigration)
        XCTAssertFalse(runner.arguments.contains(where: { $0.first == "open" }))
        guard case .failed(let failure) = controller.screen else { return XCTFail("Cancelled must have a next action") }
        XCTAssertTrue(failure.message.contains("cancelled")); XCTAssertTrue(failure.nextAction.contains("Try again"))
        await controller.start(); await controller.acceptMigration()
        XCTAssertTrue(controller.hasOpenedWorkspace)
    }
}

final class ScriptedBrowserRunner: BrowserCommandRunning {
    struct Invocation { let executable: URL; let arguments: [String]; let json: Bool }
    private var replies: [BrowserCommandResult]
    private(set) var invocations = [Invocation]()
    init(_ replies: [BrowserCommandResult]) { self.replies = replies }
    func run(executable: URL, arguments: [String], json: Bool,
             progress: @escaping @Sendable (BrowserMigrationProgress) -> Void) async throws -> BrowserCommandResult {
        invocations.append(.init(executable: executable, arguments: arguments, json: json))
        guard !replies.isEmpty else { throw BrowserHostFailure.invalidReply }
        return replies.removeFirst()
    }
    func cancelMigration() async {}
}

private final class CancellableBrowserRunner: BrowserCommandRunning {
    var arguments = [[String]]()
    var cancellations = 0
    var isMigrating: Bool { continuation != nil }
    private var continuation: CheckedContinuation<BrowserCommandResult, Never>?
    private var interruptedOnce = false
    func run(executable: URL, arguments: [String], json: Bool,
             progress: @escaping @Sendable (BrowserMigrationProgress) -> Void) async throws -> BrowserCommandResult {
        self.arguments.append(arguments)
        switch arguments.first {
        case "migration-status": return .json(#"{"ok":true,"state":"needs-handover"}"#)
        case "lifecycle":
            if interruptedOnce { return .json(#"{"ok":true,"state":"ready"}"#) }
            return await withCheckedContinuation { continuation = $0 }
        default: return .success
        }
    }
    func cancelMigration() async {
        cancellations += 1; interruptedOnce = true
        let pending = continuation; continuation = nil
        // Even a racing successful final frame cannot open after Cancel.
        pending?.resume(returning: .json(#"{"ok":true,"state":"ready"}"#))
    }
}
extension BrowserCommandResult {
    static func json(_ text: String, code: Int32 = 0) -> Self { Self(exitCode: code, response: Data(text.utf8), diagnostic: "") }
    static let success = Self(exitCode: 0, response: nil, diagnostic: "")
}
