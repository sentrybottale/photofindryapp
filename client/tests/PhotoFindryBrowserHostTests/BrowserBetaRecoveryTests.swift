import Foundation
import XCTest
@testable import PhotoFindryBrowserHost

@MainActor final class BrowserBetaRecoveryTests: XCTestCase {
    private func beta(build: Int = 108, host: String = "rust", identifier: String = "com.sentrybottale.photorecall") throws -> URL {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent("beta-recovery-" + UUID().uuidString)
        let app = root.appendingPathComponent("PhotoFindry Beta.app")
        try FileManager.default.createDirectory(at: app.appendingPathComponent("Contents/Resources/runtime"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: app.appendingPathComponent("Contents/MacOS"), withIntermediateDirectories: true)
        let info: [String: Any] = ["CFBundleIdentifier": identifier, "CFBundleExecutable": "PhotoFindry", "CFBundleShortVersionString": "0.2.0", "CFBundleVersion": String(build)]
        try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0).write(to: app.appendingPathComponent("Contents/Info.plist"))
        try JSONSerialization.data(withJSONObject: ["engine": ["implementation": "rust", "host": host, "protocol": 1, "complete": true]]).write(to: app.appendingPathComponent("Contents/Resources/runtime-manifest.json"))
        for path in ["Contents/MacOS/PhotoFindry", "Contents/Resources/runtime/PhotoWorker", "Contents/Resources/runtime/PhotoEngine"] {
            let file = app.appendingPathComponent(path)
            try Data("test fixture; never executed".utf8).write(to: file)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.path)
        }
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return app
    }

    func testOnlyCompleteCompatibleNativeBetasAreCandidates() throws {
        XCTAssertEqual(try BrowserBetaRecovery.candidate(at: beta()).build, 108)
        XCTAssertThrowsError(try BrowserBetaRecovery.candidate(at: beta(build: 107)))
        XCTAssertThrowsError(try BrowserBetaRecovery.candidate(at: beta(host: "browser")))
        XCTAssertThrowsError(try BrowserBetaRecovery.candidate(at: beta(identifier: "other.application")))
        let incomplete = try beta()
        try FileManager.default.removeItem(at: incomplete.appendingPathComponent("Contents/Resources/runtime/PhotoWorker"))
        XCTAssertThrowsError(try BrowserBetaRecovery.candidate(at: incomplete))
    }

    func testAnApplicationLinkCannotMasqueradeAsTheSavedBeta() throws {
        let app = try beta(), link = app.deletingLastPathComponent().appendingPathComponent("Alias.app")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: app)
        XCTAssertThrowsError(try BrowserBetaRecovery.candidate(at: link))
    }

    func testVerifiedBetaOpensOnlyAfterOwnedBrowserWorkDrains() async throws {
        let app = try beta(), candidate = try BrowserBetaRecovery.candidate(at: app)
        let runner = ScriptedBrowserRunner([.success])
        var steps: [String] = []
        try await BrowserBetaRecovery.handoff(to: candidate, runner: runner, prepare: { steps.append("drained") }, open: { url in
            XCTAssertEqual(url, app); XCTAssertEqual(steps, ["drained"]); steps.append("opened")
        })
        XCTAssertEqual(steps, ["drained", "opened"])
        XCTAssertEqual(runner.invocations.count, 1)
        XCTAssertEqual(runner.invocations[0].executable.path, "/usr/bin/codesign")
        XCTAssertEqual(runner.invocations[0].arguments, ["--verify", "--deep", "--strict", app.path])
    }

    func testInvalidSignatureDoesNotDrainOrOpenAnything() async throws {
        let candidate = try BrowserBetaRecovery.candidate(at: beta())
        let runner = ScriptedBrowserRunner([BrowserCommandResult(exitCode: 1, response: nil, diagnostic: "invalid")])
        var touched = false
        do {
            try await BrowserBetaRecovery.handoff(to: candidate, runner: runner, prepare: { touched = true }, open: { _ in touched = true })
            XCTFail("Unverified beta must not run")
        } catch { XCTAssertTrue(error.localizedDescription.contains("signature")) }
        XCTAssertFalse(touched)
    }

    func testBusyWorkKeepsBetaClosedAndOffersRecovery() async throws {
        let candidate = try BrowserBetaRecovery.candidate(at: beta())
        var opened = false
        do {
            try await BrowserBetaRecovery.handoff(to: candidate, runner: ScriptedBrowserRunner([.success]), prepare: {
                throw BrowserHostFailure(message: "A transfer is active.", nextAction: "Finish it, then retry.")
            }, open: { _ in opened = true })
            XCTFail("Active work must defer the beta")
        } catch { XCTAssertTrue(error.localizedDescription.contains("Finish it")) }
        XCTAssertFalse(opened)
    }

    func testPreConsentBetaChoiceDoesNotMigrateOrRestoreTheLibrary() async throws {
        let paths = BrowserHostPaths(resources: URL(fileURLWithPath: "/Applications/PhotoFindry.app/Contents/Resources"), home: URL(fileURLWithPath: "/Users/test-account"))
        let migrationRunner = ScriptedBrowserRunner([.json(#"{"ok":true,"state":"needs-handover"}"#)])
        let controller = BrowserMigrationController(paths: paths, runner: migrationRunner)
        await controller.start()
        try await BrowserBetaRecovery.handoff(to: BrowserBetaRecovery.candidate(at: beta()), runner: ScriptedBrowserRunner([.success]), prepare: {
            try await controller.prepareForExit()
        }, open: { _ in })
        XCTAssertEqual(migrationRunner.invocations.map { $0.arguments.first }, ["migration-status"])
        XCTAssertFalse(controller.hasOpenedWorkspace)
    }

    func testActualPreviousBetaWhenExplicitlySupplied() async throws {
        guard let path = ProcessInfo.processInfo.environment["PHOTOFINDRY_TEST_PREVIOUS_BETA"] else { throw XCTSkip("Supply the retained beta for actual signature verification without launching it.") }
        let candidate = try BrowserBetaRecovery.candidate(at: URL(fileURLWithPath: path))
        var opened = false
        try await BrowserBetaRecovery.handoff(to: candidate, runner: BrowserProcessRunner(), prepare: {}, open: { _ in opened = true })
        XCTAssertTrue(opened)
    }

    func testBetaHandoffPreservesDownloadedUpdateQuitGateBeforeConsent() async throws {
        let suite = "PhotoFindryBetaRecoveryTests." + UUID().uuidString
        let preferences = UserDefaults(suiteName: suite)!
        defer { preferences.removePersistentDomain(forName: suite) }
        preferences.set("111", forKey: "PhotoFindry.updateAwaitingSafeExitBuild")
        var drains = 0, ordinary = 0, opened = false, ready = false
        let updates = BrowserUpdates(info: ["CFBundleVersion": "111"], preferences: preferences, drain: {
            drains += 1
            if !ready { throw BrowserHostFailure(message: "A transfer is active.", nextAction: "Finish it, then retry.") }
        }, reopen: {}, alert: { _, _ in })
        let candidate = try BrowserBetaRecovery.candidate(at: beta())
        do {
            try await BrowserBetaRecovery.handoff(to: candidate, runner: ScriptedBrowserRunner([.success]), updates: updates,
                prepare: { ordinary += 1 }, open: { _ in opened = true })
            XCTFail("The downloaded update must retain its safe-quit gate")
        } catch { XCTAssertTrue(error.localizedDescription.contains("Finish it")) }
        XCTAssertEqual(drains, 1); XCTAssertEqual(ordinary, 0); XCTAssertFalse(opened)
        ready = true
        try await BrowserBetaRecovery.handoff(to: candidate, runner: ScriptedBrowserRunner([.success]), updates: updates,
            prepare: { ordinary += 1 }, open: { _ in opened = true })
        XCTAssertEqual(drains, 2); XCTAssertEqual(ordinary, 0); XCTAssertTrue(opened)
    }
}
