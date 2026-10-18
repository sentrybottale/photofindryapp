import AppKit
import Foundation
import UniformTypeIdentifiers

/// The app bundle remains the familiar, authenticated launch/update entry.
/// Library UI and state stay inside the bundled local companion and browser.
@MainActor final class BrowserApplicationDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var migration: BrowserMigrationController?
    private var updates: BrowserUpdates?
    private var window: NSWindow?
    private let heading = NSTextField(labelWithString: "Open PhotoFindry")
    private let detail = NSTextField(wrappingLabelWithString: "Checking the installed local workspace…")
    private let note = NSTextField(wrappingLabelWithString: "")
    private let progress = NSProgressIndicator()
    private let primary = NSButton(title: "Try again", target: nil, action: nil)
    private let secondary = NSButton(title: "Quit for now", target: nil, action: nil)
    private let recovery = NSButton(title: "Show recovery copy", target: nil, action: nil)
    private let currentBeta = NSButton(title: "Use current beta…", target: nil, action: nil)
    private var switchingToBeta = false
    private var recoveryPath: String?
    private var initialOpen: Task<Void, Never>?
    private var receivedURL = false
    private var initialRequest: BrowserLaunchRequest?
    private var installationCheck: Timer?
    private var terminating = false
    private var allowTermination = false
    private var automaticItem: NSMenuItem?
    private var installItem: NSMenuItem?
    private var updateItem: NSMenuItem?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        if let icon = Bundle.main.url(forResource: "AppIcon", withExtension: "icns").flatMap(NSImage.init(contentsOf:)) {
            NSApp.applicationIconImage = icon
        }
        createMenu(); createWindow()
        guard let resources = Bundle.main.resourceURL else {
            render(.failed(.init(message: "PhotoFindry’s installed resources are unavailable.", nextAction: "Quit and reinstall the verified application. Your library is unchanged.")))
            return
        }
        let paths = BrowserHostPaths(resources: resources, home: FileManager.default.homeDirectoryForCurrentUser)
        let controller = BrowserMigrationController(paths: paths, runner: BrowserProcessRunner())
        controller.changed = { [weak self] screen in self?.render(screen) }
        migration = controller
        let updater = BrowserUpdates(info: Bundle.main.infoDictionary ?? [:], drain: { [weak controller] in
            try await controller?.prepareForUpdate()
        }, reopen: { [weak controller] in await controller?.start() }, canShowReminder: { [weak controller] in
            controller?.hasOpenedWorkspace == true && controller?.busy != true
        }, alert: { [weak self] title, text in
            self?.showAlert(title, text)
        })
        updater.changed = { [weak self] in self?.refreshMenu() }
        updates = updater
        updater.startOnLaunch()
        // LaunchServices can deliver an incoming URL just after didFinish.
        initialOpen = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 200_000_000)
            guard !Task.isCancelled, let self else { return }
            if self.receivedURL {
                if let request = self.initialRequest { await self.migration?.start(request: request) }
            } else { await self.migration?.start() }
        }
        // The authenticated local uninstaller has already drained its owned
        // worker before moving this bundle. Retire this resident update host
        // too; never keep a deleted executable's updater alive indefinitely.
        installationCheck = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.checkInstallation() }
        }
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        receivedURL = true; initialOpen?.cancel()
        guard urls.count == 1, let request = BrowserLaunchRequest(url: urls[0]) else {
            showAlert("This PhotoFindry link is not supported", "Open PhotoFindry from its application icon or use the Open PhotoFindry button on app.photofindry.com. Links cannot select local files or change your library.")
            return
        }
        initialRequest = request
        guard updates?.preparingInstall != true, !terminating, !switchingToBeta else { return }
        guard migration != nil else { return }
        Task { await migration?.start(request: request) }
    }
    func applicationDidBecomeActive(_ notification: Notification) { checkInstallation() }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        openWorkspace(); return false
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if allowTermination { return .terminateNow }
        if switchingToBeta { return .terminateCancel }
        if migration?.busy == true {
            if let window { _ = windowShouldClose(window) }
            return .terminateCancel
        }
        guard !terminating else { return .terminateLater }
        terminating = true
        Task {
            do {
                if let updates {
                    try await updates.prepareForTermination { [weak self] in
                        try await self?.migration?.prepareForExit()
                    }
                } else { try await migration?.prepareForExit() }
                allowTermination = true
                sender.reply(toApplicationShouldTerminate: true)
            } catch {
                terminating = false
                sender.reply(toApplicationShouldTerminate: false)
                showAlert("PhotoFindry is still in use", error.localizedDescription + "\n\nOpen the local workspace and finish or pause active work in Jobs, then try quitting again.")
            }
        }
        return .terminateLater
    }
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if migration?.busy == true {
            let alert = NSAlert()
            alert.messageText = "PhotoFindry is preparing your library"
            alert.informativeText = migration?.canCancelMigration == true
                ? "Wait for preparation to finish, or cancel this handover. Cancel closes only PhotoFindry’s own preparation worker and keeps the saved library and recovery copy."
                : "PhotoFindry is finishing its current local request. Wait a moment; the window will show the next step automatically."
            alert.addButton(withTitle: "Keep waiting")
            if migration?.canCancelMigration == true { alert.addButton(withTitle: "Cancel preparation") }
            if alert.runModal() == .alertSecondButtonReturn { Task { await migration?.cancelMigration() } }
            return false
        }
        if migration?.hasOpenedWorkspace == true { sender.orderOut(nil) }
        else { NSApp.terminate(nil) }
        return false
    }

    private func createMenu() {
        let main = NSMenu()
        let application = NSMenuItem(); main.addItem(application)
        let menu = NSMenu(title: "PhotoFindry"); application.submenu = menu
        item("Open PhotoFindry", action: #selector(openWorkspace), key: "o", to: menu)
        item("Migration & recovery…", action: #selector(showSetup), to: menu)
        item("Use Current Beta…", action: #selector(useCurrentBeta), to: menu)
        menu.addItem(.separator())
        updateItem = item("Check for Updates…", action: #selector(checkUpdates), to: menu)
        automaticItem = item("Automatically Check for Updates", action: #selector(toggleAutomaticChecks), to: menu)
        installItem = item("Install Downloaded Update", action: #selector(installUpdate), to: menu)
        menu.addItem(.separator())
        item("Quit PhotoFindry", action: #selector(quit), key: "q", to: menu)
        NSApp.mainMenu = main
    }
    @discardableResult private func item(_ title: String, action: Selector, key: String = "", to menu: NSMenu) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.target = self; menu.addItem(item); return item
    }
    private func refreshMenu() {
        automaticItem?.state = updates?.automaticChecksEnabled == true ? .on : .off
        installItem?.isHidden = updates?.hasPendingInstall != true
        updateItem?.title = updates?.scheduledVersion.map { "Update Available (\($0))…" } ?? "Check for Updates…"
    }
    private func checkInstallation() {
        guard updates?.preparingInstall != true, migration?.busy != true,
              !FileManager.default.fileExists(atPath: Bundle.main.bundleURL.path) else { return }
        allowTermination = true
        NSApp.terminate(nil)
    }
    @objc private func openWorkspace() {
        guard !terminating, !switchingToBeta, updates?.preparingInstall != true else { return }
        Task { await migration?.start() }
    }
    @objc private func showSetup() {
        window?.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
    }
    @objc private func checkUpdates() { if !switchingToBeta { updates?.checkForUpdates() } }
    @objc private func toggleAutomaticChecks() { updates?.setAutomaticChecksEnabled(updates?.automaticChecksEnabled != true) }
    @objc private func installUpdate() { if !switchingToBeta { updates?.installWhenReady() } }
    @objc private func quit() { NSApp.terminate(nil) }
    @objc private func performSecondary() {
        if migration?.canCancelMigration == true { Task { await migration?.cancelMigration() } }
        else { NSApp.terminate(nil) }
    }
    @objc private func performPrimary() {
        guard let migration, !switchingToBeta else { return }
        Task {
            if case .offer = migration.screen { await migration.acceptMigration() }
            else { await migration.start() }
        }
    }
    @objc private func showRecovery() {
        guard let recoveryPath else { return }
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: recoveryPath)])
    }

    @objc private func useCurrentBeta() {
        guard !switchingToBeta, !terminating, migration?.busy != true, updates?.preparingInstall != true else { return }
        let saved = UserDefaults.standard.string(forKey: "PhotoFindryBrowserPreviousBetaPath").map { URL(fileURLWithPath: $0) }
        let preferred = BrowserBetaRecovery.preferredApplication(home: FileManager.default.homeDirectoryForCurrentUser)
        var selected = [saved, preferred].compactMap { $0 }.first { (try? BrowserBetaRecovery.candidate(at: $0)) != nil }
        if selected == nil {
            let panel = NSOpenPanel()
            panel.title = "Choose your current PhotoFindry beta"
            panel.message = "Open your saved native beta and keep using this library. This does not restore a backup or discard saved work."
            panel.prompt = "Use this beta"; panel.allowedContentTypes = [.applicationBundle]
            panel.canChooseDirectories = false; panel.canChooseFiles = true
            panel.allowsMultipleSelection = false; panel.treatsFilePackagesAsDirectories = false
            guard panel.runModal() == .OK, let url = panel.url else { return }
            selected = url
        }
        guard let selected else { return }
        switchingToBeta = true
        primary.isEnabled = false; secondary.isEnabled = false; currentBeta.isEnabled = false
        heading.stringValue = "Opening your current beta"
        detail.stringValue = "Verifying the saved application, then closing PhotoFindry’s own browser work before opening it."
        note.stringValue = "Your library stays in place. No backup is restored and no photos are retagged."
        progress.startAnimation(nil)
        Task {
            do {
                let candidate = try BrowserBetaRecovery.candidate(at: selected)
                try await BrowserBetaRecovery.handoff(to: candidate, runner: BrowserProcessRunner(), updates: self.updates, prepare: {
                    try await self.migration?.prepareForExit()
                }, open: { url in
                    if let existing = NSRunningApplication.runningApplications(withBundleIdentifier: "com.sentrybottale.photorecall")
                        .first(where: { $0.processIdentifier != ProcessInfo.processInfo.processIdentifier && $0.bundleURL?.standardizedFileURL == url.standardizedFileURL }) {
                        existing.activate(options: [])
                        return
                    }
                    let configuration = NSWorkspace.OpenConfiguration()
                    configuration.createsNewApplicationInstance = true
                    _ = try await NSWorkspace.shared.openApplication(at: url, configuration: configuration)
                })
                UserDefaults.standard.set(candidate.url.path, forKey: "PhotoFindryBrowserPreviousBetaPath")
                allowTermination = true
                NSApp.terminate(nil)
            } catch {
                switchingToBeta = false; secondary.isEnabled = true; currentBeta.isEnabled = true
                let failure = error as? BrowserHostFailure ?? BrowserHostFailure(message: "The current beta could not open.",
                    nextAction: "Choose Use current beta again and select your complete saved application. Keep your library and recovery copies in place. " + error.localizedDescription)
                render(.failed(failure))
            }
        }
    }

    private func createWindow() {
        let panel = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 620, height: 450),
            styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        panel.title = "PhotoFindry"; panel.isReleasedWhenClosed = false; panel.delegate = self
        let stack = NSStackView()
        stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = 20
        stack.translatesAutoresizingMaskIntoConstraints = false
        heading.font = .systemFont(ofSize: 26, weight: .semibold)
        detail.font = .systemFont(ofSize: 14); detail.maximumNumberOfLines = 0
        note.font = .systemFont(ofSize: 12); note.textColor = .secondaryLabelColor; note.maximumNumberOfLines = 0
        progress.style = .spinning; progress.controlSize = .small; progress.isDisplayedWhenStopped = false
        let activity = NSStackView(views: [progress, note]); activity.alignment = .centerY; activity.spacing = 12
        primary.bezelStyle = .rounded; primary.keyEquivalent = "\r"; primary.target = self; primary.action = #selector(performPrimary)
        secondary.bezelStyle = .rounded; secondary.target = self; secondary.action = #selector(performSecondary)
        recovery.bezelStyle = .rounded; recovery.target = self; recovery.action = #selector(showRecovery)
        currentBeta.bezelStyle = .rounded; currentBeta.target = self; currentBeta.action = #selector(useCurrentBeta)
        let buttons = NSStackView(views: [primary, secondary, recovery]); buttons.spacing = 10
        [heading, detail, activity, buttons, currentBeta].forEach(stack.addArrangedSubview)
        panel.contentView?.addSubview(stack)
        if let content = panel.contentView {
            NSLayoutConstraint.activate([
                stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 36),
                stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -36),
                stack.centerYAnchor.constraint(equalTo: content.centerYAnchor),
                stack.topAnchor.constraint(greaterThanOrEqualTo: content.topAnchor, constant: 24),
                stack.bottomAnchor.constraint(lessThanOrEqualTo: content.bottomAnchor, constant: -24),
                detail.widthAnchor.constraint(equalTo: stack.widthAnchor),
                activity.widthAnchor.constraint(equalTo: stack.widthAnchor)
            ])
        }
        panel.center(); window = panel
    }

    private func render(_ screen: BrowserMigrationScreen) {
        primary.isHidden = false; secondary.isHidden = false; primary.isEnabled = true
        recovery.isHidden = true; currentBeta.isHidden = true; currentBeta.isEnabled = true; progress.stopAnimation(nil)
        switch screen {
        case .checking:
            heading.stringValue = "Opening PhotoFindry"
            detail.stringValue = "Checking this Mac’s saved library and its local companion."
            note.stringValue = "Your photographs stay on this Mac."
            primary.isHidden = true; secondary.isHidden = true; progress.startAnimation(nil)
        case .offer(let existing, let disk):
            heading.stringValue = existing ? "Your library, in the browser" : "Welcome to PhotoFindry"
            detail.stringValue = existing
                ? "Switch to the browser workspace using your existing library. PhotoFindry first verifies a recovery copy, then keeps your photographs, tags, people, saved reviews and model paths in place. Existing jobs remain paused until you resume them."
                : "Create your local photo library and open it in your browser. Processing and saved data stay on this Mac. No account or upload is needed."
            note.stringValue = disk + (existing ? "\n\nSwitching is optional. Use current beta opens your saved native application without starting handover or restoring data. Quit for now leaves the library in place." : "")
            currentBeta.isHidden = !existing
            primary.title = existing ? "Switch to browser workspace" : "Create local library"
            secondary.title = "Quit for now"
        case .working(_, let message):
            heading.stringValue = "Preparing PhotoFindry"
            detail.stringValue = message
            note.stringValue = "Keep PhotoFindry open until this step finishes. It will open your workspace automatically. Original photographs are never modified."
            primary.isHidden = true; secondary.isHidden = migration?.canCancelMigration != true
            secondary.title = "Cancel preparation"; progress.startAnimation(nil)
        case .ready(let path):
            heading.stringValue = "Your local workspace is ready"
            detail.stringValue = "PhotoFindry has opened in your browser. Use this application icon or Open PhotoFindry on app.photofindry.com whenever you want to return. Resume saved processing explicitly from Jobs."
            note.stringValue = "The application keeps the local companion and authenticated updates available. Closing a browser tab does not stop your saved work."
            primary.title = "Open PhotoFindry"; secondary.title = "Quit PhotoFindry"
            currentBeta.isHidden = false
            if let path { recoveryPath = path }
            if recoveryPath == nil, let directory = migration?.paths.rollbackDirectory,
               FileManager.default.fileExists(atPath: directory.path) { recoveryPath = directory.path }
            recovery.isHidden = recoveryPath == nil
            window?.orderOut(nil)
        case .failed(let failure):
            heading.stringValue = "Let’s get PhotoFindry ready"
            detail.stringValue = failure.message
            note.stringValue = failure.nextAction
            primary.title = "Try again"; secondary.title = "Quit for now"
            currentBeta.isHidden = false
        }
        if case .ready = screen { return }
        window?.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
    }
    private func showAlert(_ title: String, _ message: String) {
        let alert = NSAlert(); alert.messageText = title; alert.informativeText = String(message.prefix(8192))
        alert.addButton(withTitle: "OK"); NSApp.activate(ignoringOtherApps: true); alert.runModal()
    }
}

@main enum PhotoFindryBrowserApplication {
    static func main() {
        let app = NSApplication.shared
        let delegate = BrowserApplicationDelegate()
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
}
