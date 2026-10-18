import AppKit
import Foundation
import Sparkle

struct BrowserUpdateConfiguration {
    static let bundleID = "com.sentrybottale.photorecall"
    static let dataContract = "photofindry-library-compatible-v1"
    static let checkInterval: TimeInterval = 6 * 60 * 60
    let feedURL: URL
    let publicKey: String
    let automaticChecksByDefault: Bool

    init?(info: [String: Any]) throws {
        guard info["SUFeedURL"] != nil || info["SUPublicEDKey"] != nil else { return nil }
        guard info["CFBundleIdentifier"] as? String == Self.bundleID,
              let text = info["SUFeedURL"] as? String, let url = URL(string: text),
              url.scheme == "https", url.host?.isEmpty == false, url.user == nil, url.password == nil,
              url.fragment == nil, url.query == nil,
              let key = info["SUPublicEDKey"] as? String, let decoded = Data(base64Encoded: key),
              decoded.count == 32, decoded.contains(where: { $0 != 0 }),
              info["SURequireSignedFeed"] as? Bool == true,
              info["SUVerifyUpdateBeforeExtraction"] as? Bool == true,
              info["SUSignedFeedFailureExpirationInterval"] as? Int == 0,
              let automatic = info["SUEnableAutomaticChecks"] as? Bool,
              info["SUAutomaticallyUpdate"] as? Bool == false,
              info["SUAllowsAutomaticUpdates"] as? Bool == false,
              info["SUSendProfileInfo"] as? Bool == false,
              info["SUEnableJavaScript"] as? Bool == false,
              info["PhotoFindryUpdateDataContract"] as? String == Self.dataContract else {
            throw BrowserHostFailure(message: "This application’s update configuration could not be verified.",
                nextAction: "Keep this copy installed and use a verified PhotoFindry download. Your library is unchanged.")
        }
        if automatic {
            guard (info["SUScheduledCheckInterval"] as? NSNumber)?.doubleValue == Self.checkInterval else {
                throw BrowserHostFailure(message: "PhotoFindry’s update schedule is invalid.", nextAction: "Reinstall a verified application. Your library is unchanged.")
            }
        }
        feedURL = url; publicKey = key; automaticChecksByDefault = automatic
    }

    static func validateCandidate(build: String, installedBuild: String, channel: String?,
                                  installationType: String, dataContract: String?, coreProtocol: String?) throws {
        guard let next = UInt64(build), let installed = UInt64(installedBuild), next > installed,
              channel == "beta", installationType == "application",
              dataContract == Self.dataContract, coreProtocol == "1" else {
            throw BrowserHostFailure(message: "This update is incompatible with this application or its library contract.",
                nextAction: "Keep using the installed application and check for an official compatible update.")
        }
    }
}

/// Sparkle may install a downloaded application during ordinary termination
/// without calling shouldPostponeRelaunchForUpdate. Keep the native host's
/// existing per-build safe-exit marker across this browser-host transition.
struct BrowserUpdateInstallationGate {
    private let preferences: UserDefaults
    private let build: String
    private let key = "PhotoFindry.updateAwaitingSafeExitBuild"
    init(preferences: UserDefaults, build: String) {
        self.preferences = preferences; self.build = build
    }
    var requiresSafeTermination: Bool { preferences.string(forKey: key) == build }
    func markDownload() { preferences.set(build, forKey: key) }
}

@MainActor protocol BrowserUpdateBackend: AnyObject {
    var automaticallyChecksForUpdates: Bool { get set }
    var automaticallyDownloadsUpdates: Bool { get set }
    var sendsSystemProfile: Bool { get set }
    func start() throws
    func checkForUpdates()
}

@MainActor private final class BrowserSparkleBackend: BrowserUpdateBackend {
    private let controller: SPUStandardUpdaterController
    init(delegate: BrowserUpdates) {
        controller = SPUStandardUpdaterController(startingUpdater: false, updaterDelegate: delegate, userDriverDelegate: delegate)
    }
    var automaticallyChecksForUpdates: Bool {
        get { controller.updater.automaticallyChecksForUpdates }
        set { controller.updater.automaticallyChecksForUpdates = newValue }
    }
    var automaticallyDownloadsUpdates: Bool {
        get { controller.updater.automaticallyDownloadsUpdates }
        set { controller.updater.automaticallyDownloadsUpdates = newValue }
    }
    var sendsSystemProfile: Bool {
        get { controller.updater.sendsSystemProfile }
        set { controller.updater.sendsSystemProfile = newValue }
    }
    func start() throws { try controller.updater.start() }
    func checkForUpdates() { controller.checkForUpdates(nil) }
}

/// Sparkle authenticates and replaces the entire app bundle, including its
/// companion payload. This host never fetches, extracts or installs code itself.
@MainActor final class BrowserUpdates: NSObject, SPUUpdaterDelegate, @preconcurrency SPUStandardUserDriverDelegate {
    private let info: [String: Any]
    private let preferences: UserDefaults
    private let backendFactory: (BrowserUpdates) -> any BrowserUpdateBackend
    private let drain: () async throws -> Void
    private let reopen: () async -> Void
    private let alert: (String, String) -> Void
    private let canShowReminder: () -> Bool
    private let installationGate: BrowserUpdateInstallationGate
    private var configuration: BrowserUpdateConfiguration?
    private var backend: (any BrowserUpdateBackend)?
    private var started = false
    private var install: (() -> Void)?
    private(set) var preparingInstall = false
    private(set) var automaticChecksEnabled = false
    private(set) var configurationError: String?
    private(set) var scheduledVersion: String?
    var hasPendingInstall: Bool { install != nil }
    var requiresSafeTermination: Bool { installationGate.requiresSafeTermination }
    var changed: (() -> Void)?

    init(info: [String: Any], preferences: UserDefaults = .standard,
         backendFactory: ((BrowserUpdates) -> any BrowserUpdateBackend)? = nil,
         drain: @escaping () async throws -> Void, reopen: @escaping () async -> Void,
         canShowReminder: @escaping () -> Bool = { true }, alert: @escaping (String, String) -> Void) {
        self.info = info; self.preferences = preferences
        self.backendFactory = backendFactory ?? { BrowserSparkleBackend(delegate: $0) }
        self.drain = drain; self.reopen = reopen; self.alert = alert
        self.canShowReminder = canShowReminder
        installationGate = BrowserUpdateInstallationGate(preferences: preferences,
            build: info["CFBundleVersion"] as? String ?? "development")
        super.init()
    }

    @discardableResult private func start() throws -> Bool {
        if let backend {
            if !started { try backend.start(); started = true; configurationError = nil }
            return true
        }
        guard let config = try BrowserUpdateConfiguration(info: info) else { return false }
        configuration = config
        let next = backendFactory(self)
        let marker = "PhotoFindry.automaticUpdateChecksEnabledDefaultV1"
        if config.automaticChecksByDefault, !preferences.bool(forKey: marker) {
            next.automaticallyChecksForUpdates = true
            preferences.set(true, forKey: marker)
        }
        next.automaticallyDownloadsUpdates = false
        next.sendsSystemProfile = false
        backend = next; automaticChecksEnabled = next.automaticallyChecksForUpdates
        try next.start(); started = true; configurationError = nil
        return true
    }
    func startOnLaunch() {
        do { _ = try start() }
        catch { configurationError = error.localizedDescription }
        changed?()
    }
    func checkForUpdates() {
        do {
            guard try start() else {
                alert("This private build has no update feed", "Public installation and update delivery are qualified separately. Install only a verified PhotoFindry application.")
                return
            }
            backend?.checkForUpdates()
        } catch { configurationError = error.localizedDescription; alert("Couldn’t check for updates", error.localizedDescription) }
    }
    func setAutomaticChecksEnabled(_ enabled: Bool) {
        do { _ = try start() } catch { configurationError = error.localizedDescription }
        backend?.automaticallyChecksForUpdates = enabled
        automaticChecksEnabled = backend?.automaticallyChecksForUpdates ?? false
        if !automaticChecksEnabled { scheduledVersion = nil }
        changed?()
    }
    func permitsUpdateCheck(_ check: SPUUpdateCheck) -> Bool {
        guard !preparingInstall else { return false }
        switch check { case .updates: return true; case .updatesInBackground, .updateInformation: return automaticChecksEnabled; @unknown default: return false }
    }
    func allowedChannels(for updater: SPUUpdater) -> Set<String> { ["beta"] }
    func feedURLString(for updater: SPUUpdater) -> String? { configuration?.feedURL.absoluteString }
    func allowedSystemProfileKeys(for updater: SPUUpdater) -> [String]? { [] }
    func updaterShouldPromptForPermissionToCheck(forUpdates updater: SPUUpdater) -> Bool { false }
    func updater(_ updater: SPUUpdater, shouldDownloadReleaseNotesForUpdate updateItem: SUAppcastItem) -> Bool { false }
    func updater(_ updater: SPUUpdater, mayPerform updateCheck: SPUUpdateCheck) throws {
        guard permitsUpdateCheck(updateCheck) else {
            throw BrowserHostFailure(message: "Automatic update checks are paused.", nextAction: "Use Check for Updates from the PhotoFindry menu.")
        }
    }
    func updater(_ updater: SPUUpdater, shouldProceedWithUpdate item: SUAppcastItem, updateCheck: SPUUpdateCheck) throws {
        try BrowserUpdateConfiguration.validateCandidate(build: item.versionString,
            installedBuild: info["CFBundleVersion"] as? String ?? "", channel: item.channel,
            installationType: item.installationType, dataContract: item.propertiesDictionary["photofindry:dataContract"] as? String,
            coreProtocol: item.propertiesDictionary["photofindry:coreProtocol"] as? String)
    }
    func updater(_ updater: SPUUpdater, willExtractUpdate item: SUAppcastItem) {
        receiveDownload()
    }
    func receiveDownload() {
        // A missing or rejected feed cannot invent a pending downloaded update.
        // An existing marker is deliberately never cleared by a later failure.
        // Configuration is installed before start(), so even a synchronous
        // authenticated download callback during updater startup is covered.
        guard configuration != nil else { return }
        installationGate.markDownload()
    }
    func prepareForTermination(ordinary: () async throws -> Void) async throws {
        if requiresSafeTermination { try await drain() }
        else { try await ordinary() }
    }
    var supportsGentleScheduledUpdateReminders: Bool { true }
    func standardUserDriverShouldHandleShowingScheduledUpdate(_ update: SUAppcastItem, andInImmediateFocus immediateFocus: Bool) -> Bool {
        immediateFocus && canShowReminder()
    }
    func standardUserDriverWillHandleShowingUpdate(_ handleShowingUpdate: Bool, forUpdate update: SUAppcastItem, state: SPUUserUpdateState) {
        guard automaticChecksEnabled, !handleShowingUpdate, !state.userInitiated else { return }
        do {
            try BrowserUpdateConfiguration.validateCandidate(build: update.versionString,
                installedBuild: info["CFBundleVersion"] as? String ?? "", channel: update.channel,
                installationType: update.installationType, dataContract: update.propertiesDictionary["photofindry:dataContract"] as? String,
                coreProtocol: update.propertiesDictionary["photofindry:coreProtocol"] as? String)
            scheduledVersion = String(update.displayVersionString.prefix(80)); changed?()
        } catch { /* An incompatible candidate is never offered by our menu. */ }
    }
    func standardUserDriverDidReceiveUserAttention(forUpdate update: SUAppcastItem) {
        scheduledVersion = nil; changed?()
    }
    func standardUserDriverWillFinishUpdateSession() {
        automaticChecksEnabled = backend?.automaticallyChecksForUpdates ?? false
        scheduledVersion = nil
        changed?()
    }
    func updater(_ updater: SPUUpdater, shouldPostponeRelaunchForUpdate item: SUAppcastItem,
                 untilInvokingBlock installHandler: @escaping () -> Void) -> Bool {
        receiveInstall(installHandler)
        return true
    }
    func receiveInstall(_ handler: @escaping () -> Void) {
        receiveDownload()
        install = handler; changed?()
        installWhenReady()
    }
    func installWhenReady() {
        guard install != nil, !preparingInstall else { return }
        preparingInstall = true; changed?()
        Task {
            do {
                try await drain()
                let callback = install; install = nil; changed?()
                callback?()
            } catch {
                preparingInstall = false; changed?()
                alert("Update is waiting for a safe handover", error.localizedDescription + "\n\nOpen PhotoFindry, finish or pause active work in Jobs, then choose Install Downloaded Update from the PhotoFindry menu.")
            }
        }
    }
    func updater(_ updater: SPUUpdater, didAbortWithError error: Error) {
        guard preparingInstall else { return }
        preparingInstall = false; install = nil; changed?()
        Task { await reopen(); alert("Update was not installed", "The current application has reopened your local workspace. Check its state before trying the update again.") }
    }
}
