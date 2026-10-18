import Foundation
import Sparkle
import XCTest
@testable import PhotoFindryBrowserHost

@MainActor final class BrowserUpdatesTests: XCTestCase {
    private var config: [String: Any] {
        ["CFBundleIdentifier": BrowserUpdateConfiguration.bundleID, "CFBundleVersion": "108",
         "SUFeedURL": "https://photofindry.example/updates.xml", "SUPublicEDKey": Data(repeating: 7, count: 32).base64EncodedString(),
         "SURequireSignedFeed": true, "SUVerifyUpdateBeforeExtraction": true, "SUSignedFeedFailureExpirationInterval": 0,
         "SUEnableAutomaticChecks": true, "SUScheduledCheckInterval": 21600,
         "SUAutomaticallyUpdate": false, "SUAllowsAutomaticUpdates": false,
         "SUSendProfileInfo": false, "SUEnableJavaScript": false,
         "PhotoFindryUpdateDataContract": BrowserUpdateConfiguration.dataContract]
    }

    func testPrivateUnconfiguredBuildDoesNotStartNetworkUpdater() throws {
        XCTAssertNil(try BrowserUpdateConfiguration(info: [:]))
        var factories = 0, alerts = 0
        let updates = BrowserUpdates(info: [:], backendFactory: { _ in factories += 1; return FakeBrowserUpdateBackend() },
            drain: {}, reopen: {}, alert: { _, _ in alerts += 1 })
        updates.startOnLaunch(); updates.checkForUpdates()
        XCTAssertEqual(factories, 0); XCTAssertEqual(alerts, 1)
    }

    func testUnsafeConfigurationFailsClosed() throws {
        XCTAssertNotNil(try BrowserUpdateConfiguration(info: config))
        for (key, value) in [
            ("CFBundleIdentifier", "other.app" as Any), ("SUFeedURL", "http://example.test/feed"),
            ("SUFeedURL", "https://example.test/feed?callback=secret"), ("SUFeedURL", "https://user@example.test/feed"),
            ("SUFeedURL", "https://example.test/feed#fragment"), ("SUPublicEDKey", Data(repeating: 0, count: 32).base64EncodedString()),
            ("SURequireSignedFeed", false), ("SUVerifyUpdateBeforeExtraction", false), ("SUSignedFeedFailureExpirationInterval", 10),
            ("SUEnableAutomaticChecks", "yes"), ("SUScheduledCheckInterval", 1), ("SUAutomaticallyUpdate", true),
            ("SUAllowsAutomaticUpdates", true), ("SUSendProfileInfo", true), ("SUEnableJavaScript", true),
            ("PhotoFindryUpdateDataContract", "silently-migrate")
        ] {
            var altered = config; altered[key] = value
            XCTAssertThrowsError(try BrowserUpdateConfiguration(info: altered), key)
        }
    }

    func testCandidateRequiresMonotonicBuildBetaWholeAppCompatibleLibraryAndCore() throws {
        func validate(_ build: String = "109", installed: String = "108", channel: String? = "beta", type: String = "application",
                      contract: String? = BrowserUpdateConfiguration.dataContract, core: String? = "1") throws {
            try BrowserUpdateConfiguration.validateCandidate(build: build, installedBuild: installed, channel: channel,
                installationType: type, dataContract: contract, coreProtocol: core)
        }
        XCTAssertNoThrow(try validate())
        for build in ["108", "107", "invalid", "18446744073709551616"] { XCTAssertThrowsError(try validate(build)) }
        XCTAssertThrowsError(try validate(channel: nil)); XCTAssertThrowsError(try validate(channel: "stable"))
        XCTAssertThrowsError(try validate(type: "package")); XCTAssertThrowsError(try validate(contract: nil))
        XCTAssertThrowsError(try validate(core: "2")); XCTAssertThrowsError(try validate(installed: "development"))
    }

    func testAutomaticOptOutSurvivesSameIdentityTransition() {
        let preferences = UserDefaults(suiteName: "PhotoFindryBrowserHostTests.\(UUID().uuidString)")!
        preferences.set(true, forKey: "PhotoFindry.automaticUpdateChecksEnabledDefaultV1")
        let backend = FakeBrowserUpdateBackend(); backend.automaticallyChecksForUpdates = false
        let updates = BrowserUpdates(info: config, preferences: preferences, backendFactory: { _ in backend }, drain: {}, reopen: {}, alert: { _, _ in })
        updates.startOnLaunch(); updates.startOnLaunch()
        XCTAssertEqual(backend.starts, 1); XCTAssertFalse(updates.automaticChecksEnabled)
        XCTAssertFalse(backend.automaticallyDownloadsUpdates); XCTAssertFalse(backend.sendsSystemProfile)
        XCTAssertFalse(updates.permitsUpdateCheck(.updatesInBackground)); XCTAssertTrue(updates.permitsUpdateCheck(.updates))
        updates.setAutomaticChecksEnabled(true)
        XCTAssertTrue(updates.permitsUpdateCheck(.updatesInBackground))
    }

    func testUpdateCannotInstallBeforeSuccessfulOwnedDrainAndExplicitRetryWorks() async {
        var attempts = 0, installs = 0, alerts = [String]()
        let updates = BrowserUpdates(info: [:], drain: {
            attempts += 1
            if attempts == 1 { throw BrowserHostFailure(message: "A tagging job is active.", nextAction: "Pause it in Jobs.") }
        }, reopen: {}, alert: { _, message in alerts.append(message) })
        updates.receiveInstall { installs += 1 }
        for _ in 0..<20 { await Task.yield() }
        XCTAssertEqual(attempts, 1); XCTAssertEqual(installs, 0)
        XCTAssertTrue(updates.hasPendingInstall); XCTAssertFalse(updates.preparingInstall)
        XCTAssertEqual(alerts.count, 1); XCTAssertTrue(alerts[0].contains("Install Downloaded Update"))
        updates.installWhenReady()
        for _ in 0..<20 { await Task.yield() }
        XCTAssertEqual(attempts, 2); XCTAssertEqual(installs, 1)
        XCTAssertFalse(updates.hasPendingInstall)
    }

    func testDownloadedUpdateBeforeConsentRoutesOrdinaryQuitThroughPackageCheckAndDefers() async throws {
        let paths = BrowserHostPaths(resources: URL(fileURLWithPath: "/Applications/PhotoFindry.app/Contents/Resources"),
            home: URL(fileURLWithPath: "/Users/migration-test"))
        let runner = ScriptedBrowserRunner([
            .json(#"{"ok":true,"state":"needs-handover"}"#),
            .json(#"{"ok":false,"ready":false,"message":"This app package has active work.","nextAction":"Pause that work and retry the update."}"#, code: 1),
            .json(#"{"ok":true,"ready":true}"#)
        ])
        let migration = BrowserMigrationController(paths: paths, runner: runner)
        await migration.start()
        let preferences = isolatedPreferences()
        let updates = BrowserUpdates(info: config, preferences: preferences,
            backendFactory: { _ in FakeBrowserUpdateBackend() }, drain: { try await migration.prepareForUpdate() },
            reopen: {}, alert: { _, _ in })
        updates.startOnLaunch(); updates.receiveDownload()
        XCTAssertTrue(updates.requiresSafeTermination)
        var ordinaryCalls = 0
        do {
            try await updates.prepareForTermination {
                ordinaryCalls += 1; try await migration.prepareForExit()
            }
            XCTFail("A downloaded update must defer ordinary quit while the package is active")
        } catch { XCTAssertTrue(error.localizedDescription.contains("Pause that work")) }
        XCTAssertEqual(ordinaryCalls, 0)
        XCTAssertEqual(runner.invocations.last?.arguments, ["prepare-app-update", "--data-dir", paths.library.path])
        XCTAssertFalse(migration.hasOpenedWorkspace)
        try await updates.prepareForTermination { ordinaryCalls += 1 }
        XCTAssertEqual(ordinaryCalls, 0)
        XCTAssertEqual(runner.invocations.count, 3)
    }

    func testDownloadGateSurvivesSameBuildControllerRecreationAndNextBuildDoesNotInheritIt() async throws {
        let preferences = isolatedPreferences()
        var firstDrains = 0, sameDrains = 0, nextDrains = 0, ordinary = 0
        let first = BrowserUpdates(info: config, preferences: preferences,
            backendFactory: { _ in FakeBrowserUpdateBackend() }, drain: { firstDrains += 1 }, reopen: {}, alert: { _, _ in })
        first.startOnLaunch(); first.receiveDownload()
        XCTAssertEqual(preferences.string(forKey: "PhotoFindry.updateAwaitingSafeExitBuild"), "108")
        let same = BrowserUpdates(info: config, preferences: preferences,
            backendFactory: { _ in FakeBrowserUpdateBackend() }, drain: { sameDrains += 1 }, reopen: {}, alert: { _, _ in })
        XCTAssertTrue(same.requiresSafeTermination, "Ordinary quit is guarded before this controller starts checking feeds")
        try await same.prepareForTermination { ordinary += 1 }
        XCTAssertEqual(sameDrains, 1); XCTAssertEqual(ordinary, 0)
        var nextConfig = config; nextConfig["CFBundleVersion"] = "109"
        let next = BrowserUpdates(info: nextConfig, preferences: preferences,
            backendFactory: { _ in FakeBrowserUpdateBackend() }, drain: { nextDrains += 1 }, reopen: {}, alert: { _, _ in })
        XCTAssertFalse(next.requiresSafeTermination)
        try await next.prepareForTermination { ordinary += 1 }
        XCTAssertEqual(nextDrains, 0); XCTAssertEqual(ordinary, 1); XCTAssertEqual(firstDrains, 0)
    }

    func testNoDownloadKeepsQuitForNowNonAdoptingAndInvalidFeedCannotInventGate() async throws {
        var rejected = config; rejected["SURequireSignedFeed"] = false
        for info in [[:], rejected, config] {
            let preferences = isolatedPreferences()
            var drains = 0, ordinary = 0
            let updates = BrowserUpdates(info: info, preferences: preferences,
                backendFactory: { _ in FakeBrowserUpdateBackend() }, drain: { drains += 1 }, reopen: {}, alert: { _, _ in })
            updates.startOnLaunch()
            if info["SURequireSignedFeed"] as? Bool != true { updates.receiveDownload() }
            XCTAssertFalse(updates.requiresSafeTermination)
            XCTAssertNil(preferences.object(forKey: "PhotoFindry.updateAwaitingSafeExitBuild"))
            try await updates.prepareForTermination { ordinary += 1 }
            XCTAssertEqual(drains, 0); XCTAssertEqual(ordinary, 1)
        }
    }

    func testPendingInstallAlsoMarksPersistentGateBeforeOwnedDrain() async {
        let preferences = isolatedPreferences()
        var drains = 0, installs = 0
        let updates = BrowserUpdates(info: config, preferences: preferences,
            backendFactory: { _ in FakeBrowserUpdateBackend() }, drain: {
                drains += 1
                throw BrowserHostFailure(message: "Still active", nextAction: "Pause work first")
            }, reopen: {}, alert: { _, _ in })
        updates.startOnLaunch()
        updates.receiveInstall { installs += 1 }
        XCTAssertTrue(updates.requiresSafeTermination)
        for _ in 0..<20 { await Task.yield() }
        XCTAssertEqual(drains, 1); XCTAssertEqual(installs, 0)
        XCTAssertTrue(updates.hasPendingInstall)
        XCTAssertTrue(BrowserUpdateInstallationGate(preferences: preferences, build: "108").requiresSafeTermination)
    }

    func testSynchronousAuthenticatedDownloadCallbackDuringUpdaterStartStillMarksQuitGate() {
        let preferences = isolatedPreferences()
        let backend = FakeBrowserUpdateBackend()
        let updates = BrowserUpdates(info: config, preferences: preferences,
            backendFactory: { _ in backend }, drain: {}, reopen: {}, alert: { _, _ in })
        backend.onStart = { updates.receiveDownload() }
        defer { backend.onStart = nil }
        updates.startOnLaunch()
        XCTAssertEqual(backend.starts, 1)
        XCTAssertTrue(updates.requiresSafeTermination)
    }

    private func isolatedPreferences() -> UserDefaults {
        let name = "PhotoFindryBrowserHostTests.\(UUID().uuidString)"
        let preferences = UserDefaults(suiteName: name)!
        addTeardownBlock { preferences.removePersistentDomain(forName: name) }
        return preferences
    }
}

@MainActor final class FakeBrowserUpdateBackend: BrowserUpdateBackend {
    var automaticallyChecksForUpdates = false
    var automaticallyDownloadsUpdates = true
    var sendsSystemProfile = true
    var starts = 0
    var onStart: (() -> Void)?
    func start() throws { starts += 1; onStart?() }
    func checkForUpdates() {}
}
