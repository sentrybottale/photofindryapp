// Minimal installed macOS URL broker. All library UI remains in the browser.
// Package beside the bundled photofindry client executable; never download code here.
import AppKit
import Foundation
import CoreServices

enum NativeLaunchRequest: Equatable {
    case companion
    case workspace
    case archive(String)
}
func archiveIdentity(_ value: String) -> String? {
    guard value.utf8.count==36 else {return nil}
    let bytes=Array(value.utf8)
    guard bytes.enumerated().allSatisfy({ index, byte in
        [8,13,18,23].contains(index) ? byte==45 : (48...57).contains(byte)||(65...70).contains(byte)||(97...102).contains(byte)
    }) else { return nil }
    return value.lowercased()
}
func nativeLaunchRequest(_ value: String) -> NativeLaunchRequest? {
    if value=="photofindry-companion://open" { return .companion }
    if value=="photofindry-local://open" { return .workspace }
    let prefix="photofindry://archive/v1/"
    guard value.hasPrefix(prefix), let id=archiveIdentity(String(value.dropFirst(prefix.count))) else {return nil}
    return .archive(id)
}
func recoveryOfferAvailable(version: URL, attempted: Bool) -> Bool {
    version.deletingLastPathComponent().lastPathComponent == "versions"
        && UInt64(version.lastPathComponent) != nil
        && !attempted
}

final class PhotoFindryLauncher: NSObject, NSApplicationDelegate {
    private var launch: Process?
    private var hasURL = false
    private var scheduled: DispatchWorkItem?
    private var activeRequest: NativeLaunchRequest?
    private var pendingRequest: NativeLaunchRequest?
    private var retryRequest: NativeLaunchRequest = .companion
    private var recoveryAttempted = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        // LaunchServices delivers an incoming URL around launch completion. A
        // normal Finder launch still opens the registered library once.
        let work = DispatchWorkItem { [weak self] in
            guard let self, !self.hasURL else { return }
            self.openLibrary()
        }
        scheduled = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2, execute: work)
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        hasURL = true
        scheduled?.cancel()
        guard urls.count == 1, let request=nativeLaunchRequest(urls[0].absoluteString) else {
            showError("This link is not supported. Use Open PhotoFindry to open your registered local library.")
            return
        }
        openLibrary(request)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        openLibrary()
        return false
    }

    private func openLibrary(_ request: NativeLaunchRequest = .companion, recovering: Bool = false) {
        if launch != nil {
            if request != activeRequest { pendingRequest=request }
            return
        }
        retryRequest = request
        let executable = Bundle.main.bundleURL.deletingLastPathComponent().appendingPathComponent("photofindry")
        var info: stat = stat()
        guard executable.withUnsafeFileSystemRepresentation({ pointer in
            guard let pointer else { return false }
            return lstat(pointer, &info) == 0 && (info.st_mode & S_IFMT) == S_IFREG && (info.st_mode & 0o111) != 0
        }) else {
            showError("The installed PhotoFindry companion is missing. Reinstall the trusted package.")
            return
        }
        let process = Process()
        process.executableURL = executable
        if recovering { process.arguments=["lifecycle","recover"] }
        else { switch request {
        case .companion: process.arguments=["open-companion"]
        case .workspace: process.arguments=["open"]
        case .archive(let id): process.arguments=["open-archive",id]
        }}
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        // The CLI reports bounded human-readable errors, never a bootstrap URL.
        let errors = Pipe()
        process.standardError = errors
        let captured = BoundedError()
        errors.fileHandleForReading.readabilityHandler = { handle in
            let bytes = handle.availableData
            captured.append(bytes)
            if bytes.isEmpty { handle.readabilityHandler = nil }
        }
        launch = process
        activeRequest=request
        process.terminationHandler = { [weak self] process in
            let message = captured.message()
            DispatchQueue.main.async {
                guard let self else { return }
                self.launch = nil
                self.activeRequest=nil
                if recovering { self.recoveryAttempted = true }
                if process.terminationStatus == 0 {
                    if recovering {self.openLibrary(self.pendingRequest ?? request);self.pendingRequest=nil;return}
                    if let next=self.pendingRequest {self.pendingRequest=nil;self.openLibrary(next)}
                    else {NSApp.terminate(nil)}
                }
                else { self.showError(message?.isEmpty == false ? message! : "PhotoFindry could not open its local library. Open the installed setup launcher to review the registration.") }
            }
        }
        do { try process.run() }
        catch {
            launch = nil
            activeRequest=nil;pendingRequest=nil
            showError("PhotoFindry could not start its installed local companion.")
        }
    }

    private func showError(_ message: String) {
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "PhotoFindry could not open"
        alert.informativeText = String(message.prefix(4096))
        alert.alertStyle = .warning
        let bin=Bundle.main.bundleURL.deletingLastPathComponent()
        let version=bin.deletingLastPathComponent().deletingLastPathComponent()
        let canRecover = recoveryOfferAvailable(version: version, attempted: recoveryAttempted)
        if canRecover {
            alert.addButton(withTitle: "Recover and open")
            alert.addButton(withTitle: "Cancel")
            alert.informativeText += "\n\nRecovery restarts verified companion software. Your current library stays in place."
            if alert.runModal() == .alertFirstButtonReturn {
                openLibrary(retryRequest,recovering:true)
                return
            }
        } else {
            if recoveryAttempted && !message.contains("PF-OPEN-300") { alert.informativeText += "\n\nReinstall the latest verified PhotoFindry companion from photofindry.com, then open it again. Keep your library and recovery copy in place." }
            alert.addButton(withTitle: "OK");alert.runModal()
        }
        if launch == nil { NSApp.terminate(nil) }
    }
}

// Keep diagnostics bounded while continuously draining the child pipe.
final class BoundedError: @unchecked Sendable {
    private let lock = NSLock()
    private var bytes = Data()
    func append(_ data: Data) {
        lock.lock(); defer { lock.unlock() }
        bytes.append(data.prefix(max(0, 4096 - bytes.count)))
    }
    func message() -> String? {
        lock.lock(); defer { lock.unlock() }
        return String(data: bytes, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

// The companion passes the short-lived launch capability over a private pipe,
// never through process arguments, environment variables or an on-disk file.
// This internal mode has no URL-handler entry point and accepts no destination.
func localWorkspaceURL(_ input: Data) -> URL? {
    guard !input.isEmpty, input.count <= 1024, let text = String(data: input, encoding: .utf8) else { return nil }
    // Narrow JSON excludes duplicate/unknown keys, booleans, fractional ports,
    // escapes, callback URLs and extra documents. Order is immaterial.
    let whitespace = #"[ \t\r\n]*"#
    let field = #""(?:port|ticket|archiveId)""# + whitespace + ":" + whitespace + #"(?:[1-9][0-9]{0,4}|"[0-9A-Fa-f-]{1,64}")"#
    let pattern = #"\A"# + whitespace + #"\{"# + whitespace + field
        + "(?:" + whitespace + "," + whitespace + field + "){1,2}" + whitespace + #"\}"# + whitespace + #"\z"#
    let fullRange=NSRange(location:0,length:(text as NSString).length)
    guard let expression=try? NSRegularExpression(pattern:pattern), expression.firstMatch(in:text,range:fullRange) != nil,
          let object=(try? JSONSerialization.jsonObject(with:input)) as? [String:Any],
          let keyExpression=try? NSRegularExpression(pattern:#""(?:port|ticket|archiveId)"[ \t\r\n]*:"#),
          keyExpression.numberOfMatches(in:text,range:fullRange)==object.count,
          Set(object.keys)==Set(["port","ticket"]) || Set(object.keys)==Set(["port","ticket","archiveId"]),
          let port=object["port"] as? NSNumber, (1...65535).contains(port.intValue),
          let ticket=object["ticket"] as? String, ticket.utf8.count==64,
          ticket.utf8.allSatisfy({(48...57).contains($0)||(97...102).contains($0)}) else { return nil }
    var suffix=""
    if let archive=object["archiveId"] {
        guard let text=archive as? String, let id=archiveIdentity(text) else {return nil}
        suffix="&archive=\(id)"
    }
    return URL(string:"http://127.0.0.1:\(port.intValue)/#bootstrap=\(ticket)\(suffix)")
}
func openLocalWorkspaceFromInput() -> Bool {
    var input = Data()
    do {
        while input.count <= 1024 {
            guard let part = try FileHandle.standardInput.read(upToCount: 1025 - input.count), !part.isEmpty else { break }
            input.append(part)
        }
    } catch { return false }
    guard let url=localWorkspaceURL(input) else {return false}
    return NSWorkspace.shared.open(url)
}

final class LinkRegistrationResult: @unchecked Sendable {
    private let lock=NSLock()
    private var result:Bool?
    func finish(_ succeeded:Bool) {lock.lock();defer{lock.unlock()};result=succeeded}
    func value()->Bool? {lock.lock();defer{lock.unlock()};return result}
}
typealias LinkHandlerSetter = (URL,String,@escaping (Error?)->Void)->Void

// Injected functions allow qualification without changing macOS preferences.
// The completion can arrive on the main queue, so a semaphore wait would deadlock.
let companionLinks=[("photofindry-companion","photofindry-companion://open"),
                    ("photofindry-local","photofindry-local://open"),
                    ("photofindry","photofindry://archive/v1/00000000-0000-0000-0000-000000000000")]
func selectLocalLinkHandlers(_ application:URL, timeout:TimeInterval=30,
                            links:[(String,String)]=companionLinks,
                            setHandler:LinkHandlerSetter, resolve:(URL)->URL?) -> Bool {
    let expected=application.resolvingSymlinksInPath().standardizedFileURL
    func yieldToCallbacks() {
        if !RunLoop.current.run(mode:.default,before:Date(timeIntervalSinceNow:0.025)) {
            Thread.sleep(forTimeInterval:0.005)
        }
    }
    for (scheme,probe) in links {
        let completion=LinkRegistrationResult()
        let deadline=ProcessInfo.processInfo.systemUptime+max(0,timeout)
        setHandler(expected,scheme) {completion.finish($0==nil)}
        while completion.value()==nil && ProcessInfo.processInfo.systemUptime<deadline {yieldToCallbacks()}
        guard completion.value()==true,let url=URL(string:probe) else {return false}
        // Allow LaunchServices' lookup cache to observe the completed change.
        // A different installed copy never counts as successful registration.
        var matched=false
        repeat {
            matched=resolve(url)?.resolvingSymlinksInPath().standardizedFileURL==expected
            if matched {break}
            yieldToCallbacks()
        } while ProcessInfo.processInfo.systemUptime<deadline
        guard matched else {return false}
    }
    return true
}

// Link ownership belongs to the short-lived helper, never its enclosing installer.
func localLauncherApplication(_ executable:String) -> URL? {
    let suffix="/CompanionLauncher.app/Contents/MacOS/CompanionLauncher"
    if executable.hasSuffix(suffix) {
        return URL(fileURLWithPath:String(executable.dropLast("/Contents/MacOS/CompanionLauncher".count)))
    }
    guard executable.hasSuffix("/CompanionLauncher") else {return nil}
    return URL(fileURLWithPath:executable).deletingLastPathComponent().appendingPathComponent("CompanionLauncher.app")
}
// Kept for safe removal of legacy browser app layouts, never link ownership.
func enclosingBrowserApplication(_ executable: String) -> String? {
    for suffix in ["/Contents/Resources/companion/bin/CompanionLauncher",
                   "/Contents/Resources/companion/bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher"] {
        if executable.hasSuffix(suffix) {
            let root=String(executable.dropLast(suffix.count))
            if root.hasSuffix(".app"),uninstallPath(root) {return root}
        }
    }
    return nil
}
func verifiedBrowserApplication(_ path: String) -> Bool {
    guard realDirectoryChain(path,inspect:nativeUninstallIdentity) != nil else {return false}
    for relative in ["/Contents","/Contents/MacOS","/Contents/Resources"] {
        guard nativeUninstallIdentity(path+relative)?.isDirectory == true else {return false}
    }
    for relative in ["/Contents/Info.plist","/Contents/Resources/browser-host-manifest.json"] {
        guard let info=nativeUninstallIdentity(path+relative),
              info.mode & UInt32(S_IFMT) == UInt32(S_IFREG),info.size>0,info.size<=32768 else {return false}
    }
    guard nativeUninstallIdentity(path+"/Contents/MacOS/PhotoFindry")?.isExecutable == true,
          let plist=NSDictionary(contentsOfFile:path+"/Contents/Info.plist"),
          plist["CFBundleIdentifier"] as? String == "com.sentrybottale.photorecall",
          plist["CFBundleExecutable"] as? String == "PhotoFindry",
          let data=FileManager.default.contents(atPath:path+"/Contents/Resources/browser-host-manifest.json"),
          let manifest=(try? JSONSerialization.jsonObject(with:data)) as? [String:Any],
          manifest["schema"] as? Int == 1,manifest["kind"] as? String == "browser-host",
          manifest["bundleIdentifier"] as? String == "com.sentrybottale.photorecall",
          manifest["companion"] as? String == "companion" else {return false}
    let verifier=Process();verifier.executableURL=URL(fileURLWithPath:"/usr/bin/codesign")
    verifier.arguments=["--verify","--deep","--strict",path]
    verifier.standardInput=FileHandle.nullDevice;verifier.standardOutput=FileHandle.nullDevice;verifier.standardError=FileHandle.nullDevice
    let ended=DispatchSemaphore(value:0);verifier.terminationHandler={_ in ended.signal()}
    do {try verifier.run()} catch {return false}
    if ended.wait(timeout:.now()+30) != .success {verifier.terminate();return false}
    return verifier.terminationStatus == 0
}

func registerLocalLinks() -> Bool {
    let executable=URL(fileURLWithPath:CommandLine.arguments[0]).standardizedFileURL
    guard let expected=localLauncherApplication(executable.path),
          let info=NSDictionary(contentsOfFile:expected.appendingPathComponent("Contents/Info.plist").path),
          info["CFBundleIdentifier"] as? String == "com.sentrybottale.photorecall.companion-launcher",
          info["CFBundleExecutable"] as? String == "CompanionLauncher",
          info["LSUIElement"] as? Bool == true else {return false}
    guard LSRegisterURL(expected as CFURL,true)==noErr else {return false}
    var directory:ObjCBool=false
    guard FileManager.default.fileExists(atPath:expected.path,isDirectory:&directory),directory.boolValue else {return false}
    return selectLocalLinkHandlers(expected,setHandler:{ application,scheme,completion in
        NSWorkspace.shared.setDefaultApplication(at:application,toOpenURLsWithScheme:scheme,completion:completion)
    },resolve:{NSWorkspace.shared.urlForApplication(toOpen:$0)})
}

// These private stdin modes are never reachable through a URL. The backend
// separately verifies the complete package manifest and stops owned processes.
// This helper only confirms locally, then moves its own exact package to Trash.
struct NativeUninstallRequest: Equatable {
    let packagePath: String
    let libraryPath: String
    let interactive: Bool
}
struct NativeUninstallIdentity: Equatable {
    let device: UInt64
    let inode: UInt64
    let mode: UInt32
    let owner: UInt32
    let size: Int64
    let modified: Int64
    var modifiedNanoseconds: Int64 = 0
    var changed: Int64 = 0
    var changedNanoseconds: Int64 = 0
    var isDirectory: Bool { mode & UInt32(S_IFMT) == UInt32(S_IFDIR) }
    var isExecutable: Bool { mode & UInt32(S_IFMT) == UInt32(S_IFREG) && mode & 0o111 != 0 }
}
typealias NativeUninstallInspector = (String) -> NativeUninstallIdentity?

func uninstallPath(_ value: String) -> Bool {
    guard value.utf8.count <= 4096, value.hasPrefix("/"), value != "/",
          !value.hasSuffix("/"), !value.contains("//"),
          value.unicodeScalars.allSatisfy({ !CharacterSet.controlCharacters.contains($0) }),
          !value.split(separator:"/").contains(where: { $0 == "." || $0 == ".." }) else { return false }
    // Foundation standardization can rewrite existing /private/var paths to
    // their /var symlink spelling. Keep the caller's lexical spelling intact;
    // lstat ancestry and physical identities establish filesystem authority.
    return true
}
func nativeUninstallObject(_ input: Data, expected: Set<String>) -> [String:Any]? {
    guard !input.isEmpty, input.count <= 32768,
          let text=String(data:input,encoding:.utf8) else { return nil }
    let whitespace=#"[ \t\r\n]*"#
    // JSON strings may contain escaped Unicode and spaces; keys remain literal.
    // Matching each member before decoding prevents JSON's duplicate-key collapse.
    let string=#""(?:[^"\\\x00-\x1f]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*""#
    let member=#""(?:packagePath|libraryPath|interactive|message)""#+whitespace+":"+whitespace+"(?:"+string+"|true|false)"
    let pattern=#"\A"#+whitespace+#"\{"#+whitespace+member+"(?:"+whitespace+","+whitespace+member+"){0,2}"+whitespace+#"\}"#+whitespace+#"\z"#
    let range=NSRange(location:0,length:(text as NSString).length)
    guard let expression=try? NSRegularExpression(pattern:pattern),
          expression.firstMatch(in:text,range:range) != nil,
          let keys=try? NSRegularExpression(pattern:#""(?:packagePath|libraryPath|interactive|message)"[ \t\r\n]*:"#),
          let object=(try? JSONSerialization.jsonObject(with:input)) as? [String:Any],
          keys.numberOfMatches(in:text,range:range)==object.count,
          Set(object.keys)==expected else {return nil}
    return object
}
func nativeUninstallRequest(_ input: Data, confirmation: Bool) -> NativeUninstallRequest? {
    guard let object=nativeUninstallObject(input,expected:Set(confirmation ? ["packagePath","libraryPath"] : ["packagePath","libraryPath","interactive"])),
          let package=object["packagePath"] as? String,uninstallPath(package),
          let library=object["libraryPath"] as? String,uninstallPath(library),
          confirmation || object["interactive"] is Bool else { return nil }
    return NativeUninstallRequest(packagePath:package,libraryPath:library,interactive:object["interactive"] as? Bool ?? false)
}
func nativeUninstallErrorRequest(_ input:Data) -> (request:NativeUninstallRequest,message:String)? {
    guard let object=nativeUninstallObject(input,expected:["packagePath","libraryPath","message"]),
          let package=object["packagePath"] as? String,uninstallPath(package),
          let library=object["libraryPath"] as? String,uninstallPath(library),
          let message=object["message"] as? String,!message.isEmpty,message.utf8.count <= 2048,
          message.unicodeScalars.allSatisfy({ !CharacterSet.controlCharacters.contains($0) }) else {return nil}
    return (NativeUninstallRequest(packagePath:package,libraryPath:library,interactive:true),message)
}
func nativeUninstallIdentity(_ path: String) -> NativeUninstallIdentity? {
    var info=stat()
    guard path.withCString({lstat($0,&info)}) == 0 else { return nil }
    return NativeUninstallIdentity(device:UInt64(UInt32(bitPattern:info.st_dev)),inode:UInt64(info.st_ino),
        mode:UInt32(info.st_mode),owner:info.st_uid,size:info.st_size,
        modified:Int64(info.st_mtimespec.tv_sec),modifiedNanoseconds:Int64(info.st_mtimespec.tv_nsec),
        changed:Int64(info.st_ctimespec.tv_sec),changedNanoseconds:Int64(info.st_ctimespec.tv_nsec))
}
func pathContains(_ parent: String, _ child: String) -> Bool {
    child == parent || child.hasPrefix(parent+"/")
}
func realDirectoryChain(_ path: String, inspect: NativeUninstallInspector) -> [NativeUninstallIdentity]? {
    var current="",chain=[NativeUninstallIdentity]()
    for component in path.split(separator:"/") {
        current += "/"+component
        guard let identity=inspect(current),identity.isDirectory else { return nil }
        chain.append(identity)
    }
    return chain
}
func uninstallPackageIdentities(_ request: NativeUninstallRequest, executingPath: String,
                                owner: UInt32, inspect: NativeUninstallInspector) -> [String:NativeUninstallIdentity]? {
    let root=request.packagePath
    if let outer=enclosingBrowserApplication(executingPath),outer != root {return nil}
    let browser=enclosingBrowserApplication(executingPath) == root
    let companion=browser ? root+"/Contents/Resources/companion" : root
    let bin=companion+"/bin",bundle=bin+"/CompanionLauncher.app"
    let helpers=[bin+"/CompanionLauncher",bundle+"/Contents/MacOS/CompanionLauncher"]
    guard uninstallPath(executingPath),helpers.contains(executingPath),
          let packageChain=realDirectoryChain(root,inspect:inspect),let packageIdentity=packageChain.last else { return nil }
    let library=request.libraryPath
    guard !pathContains(root,library),!pathContains(library,root),
          let libraryChain=realDirectoryChain(library,inspect:inspect),let libraryIdentity=libraryChain.last else { return nil }
    // APFS can resolve differently cased spellings to the same directories.
    // Check physical ancestry as well as the human-readable path boundaries.
    func sameDirectory(_ left:NativeUninstallIdentity,_ right:NativeUninstallIdentity)->Bool {
        left.device==right.device && left.inode==right.inode
    }
    guard !libraryChain.contains(where:{sameDirectory($0,packageIdentity)}),
          !packageChain.contains(where:{sameDirectory($0,libraryIdentity)}) else { return nil }
    var identities=[String:NativeUninstallIdentity]()
    let outerDirectories=browser ? [root+"/Contents",root+"/Contents/MacOS",root+"/Contents/Resources",companion] : []
    for path in [root,bin,bundle,bundle+"/Contents",bundle+"/Contents/MacOS"]+outerDirectories {
        guard let info=inspect(path),info.isDirectory,info.owner == owner else { return nil }
        identities[path]=info
    }
    for path in [bin+"/photofindry"]+helpers+(browser ? [root+"/Contents/MacOS/PhotoFindry"] : []) {
        guard let info=inspect(path),info.isExecutable,info.owner == owner else { return nil }
        identities[path]=info
    }
    if browser {
        for path in [root+"/Contents/Info.plist",root+"/Contents/Resources/browser-host-manifest.json"] {
            guard let info=inspect(path),info.mode & UInt32(S_IFMT) == UInt32(S_IFREG),
                  info.owner==owner,info.size>0,info.size<=32768 else {return nil}
            identities[path]=info
        }
    }
    // These are source data, not package payloads. The manifest validator in the
    // companion checks every other entry before this helper is invoked.
    for name in ["catalog.sqlite","catalog.json","people.sqlite","people.json","evidence-memory.sqlite","jobs.sqlite","Library"] {
        guard [root,bin,companion].allSatisfy({inspect($0+"/"+name) == nil}) else { return nil }
    }
    return identities
}
func confirmNativeUninstall(_ input: Data, executingPath: String, owner: UInt32,
                            inspect: NativeUninstallInspector, present: (NativeUninstallRequest)->Bool) -> Bool? {
    guard let request=nativeUninstallRequest(input,confirmation:true),
          uninstallPackageIdentities(request,executingPath:executingPath,owner:owner,inspect:inspect) != nil else { return nil }
    return present(request)
}
func trashNativePackage(_ input: Data, executingPath: String, owner: UInt32,
                        inspect: NativeUninstallInspector, moveToTrash: (URL)->URL?) -> URL? {
    guard let request=nativeUninstallRequest(input,confirmation:false),
          let identities=uninstallPackageIdentities(request,executingPath:executingPath,owner:owner,inspect:inspect),
          uninstallPackageIdentities(request,executingPath:executingPath,owner:owner,inspect:inspect)==identities,
          let destination=moveToTrash(URL(fileURLWithPath:request.packagePath)),
          destination.isFileURL,uninstallPath(destination.path),destination.path != request.packagePath else { return nil }
    return destination
}
func readUninstallInput() -> Data? {
    var input=Data()
    do {
        while input.count <= 32768 {
            guard let part=try FileHandle.standardInput.read(upToCount:32769-input.count),!part.isEmpty else { break }
            input.append(part)
        }
    } catch { return nil }
    return input.count <= 32768 ? input : nil
}
func writeNativeUninstallReply(_ reply:[String:Any],closeOutput:Bool) -> Bool {
    guard let output=try? JSONSerialization.data(withJSONObject:reply,options:[.sortedKeys]) else { return false }
    FileHandle.standardOutput.write(output)
    FileHandle.standardOutput.write(Data([10]))
    if closeOutput {try? FileHandle.standardOutput.close()}
    return true
}
func runNativeUninstall(confirmation: Bool) -> Bool {
    guard let input=readUninstallInput(),let request=nativeUninstallRequest(input,confirmation:confirmation) else { return false }
    let executingPath=CommandLine.arguments[0]
    // Initialize AppKit before moving its on-disk helper bundle.
    if request.interactive {
        let app=NSApplication.shared
        app.setActivationPolicy(.accessory)
    }
    func resultAlert(_ title:String,_ message:String) {
        guard request.interactive else {return}
        NSApplication.shared.activate(ignoringOtherApps:true)
        let alert=NSAlert()
        alert.messageText=title;alert.informativeText=message
        alert.addButton(withTitle:"OK");alert.runModal()
    }
    let reply:[String:Any]
    var completedNotice:String?
    if confirmation {
        guard let confirmed=confirmNativeUninstall(input,executingPath:executingPath,owner:getuid(),inspect:nativeUninstallIdentity,present:{ request in
            let app=NSApplication.shared
            app.setActivationPolicy(.accessory)
            app.activate(ignoringOtherApps:true)
            let alert=NSAlert()
            alert.messageText="Uninstall PhotoFindry companion?"
            alert.informativeText="The installed program will move to Trash. Your saved library, original photos, model files and settings will remain. Running jobs will stop; saved progress will remain.\n\nProgram: \(request.packagePath)\nLibrary kept: \(request.libraryPath)"
            alert.alertStyle = .warning
            alert.addButton(withTitle:"Cancel")
            alert.addButton(withTitle:"Uninstall")
            alert.buttons[0].keyEquivalent="\r"
            alert.buttons[1].keyEquivalent=""
            return alert.runModal() == .alertSecondButtonReturn
        }) else { return false }
        reply=["confirmed":confirmed]
    } else {
        guard let destination=trashNativePackage(input,executingPath:executingPath,owner:getuid(),inspect:nativeUninstallIdentity,moveToTrash:{ source in
            var destination:NSURL?
            do { try FileManager.default.trashItem(at:source,resultingItemURL:&destination) }
            catch { return nil }
            return destination as URL?
        }) else {
            _=writeNativeUninstallReply(["trashed":false],closeOutput:request.interactive)
            resultAlert("PhotoFindry could not finish uninstalling", "The package could not be moved to Trash. No files were permanently deleted. Your saved library remains at:\n\n\(request.libraryPath)\n\nKeep the installed package in place and retry from its bin/photofindry command.")
            return false
        }
        reply=["trashed":true,"path":destination.path]
        completedNotice="The program is now in Trash. Your photos, models, settings and saved progress remain.\n\nLibrary kept: \(request.libraryPath)\n\nTo return, restore the program from Trash and register this library again."
    }
    // The parent records completion before this optional acknowledgement. A
    // dismissed-later alert must not leave uninstall looking incomplete.
    guard writeNativeUninstallReply(reply,closeOutput:request.interactive) else {return false}
    if let message=completedNotice {resultAlert("PhotoFindry companion uninstalled",message)}
    return true
}

func runNativeUninstallError() -> Bool {
    guard let input=readUninstallInput(),let details=nativeUninstallErrorRequest(input) else {return false}
    let executingPath=CommandLine.arguments[0]
    guard uninstallPackageIdentities(details.request,executingPath:executingPath,owner:getuid(),inspect:nativeUninstallIdentity) != nil else {return false}
    let app=NSApplication.shared
    app.setActivationPolicy(.accessory)
    app.activate(ignoringOtherApps:true)
    let alert=NSAlert()
    alert.messageText="PhotoFindry could not finish uninstalling"
    alert.informativeText=details.message+"\n\nNo files were permanently deleted. Your saved library, photos, model files and settings remain.\n\nLibrary kept: \(details.request.libraryPath)\n\nKeep the installed package in place and retry from its bin/photofindry command."
    alert.alertStyle = .warning
    alert.addButton(withTitle:"OK")
    guard writeNativeUninstallReply(["shown":true],closeOutput:true) else {return false}
    alert.runModal()
    return true
}

// The CLI may retire only the verified migration bundle recorded at handoff.
// The website cannot supply this request; it arrives on the local private pipe.
func retireMigrationApplication() -> Bool {
    guard let input=readUninstallInput(),
          let request=nativeUninstallRequest(input,confirmation:true),
          !pathContains(request.packagePath,request.libraryPath),
          !pathContains(request.libraryPath,request.packagePath),
          verifiedBrowserApplication(request.packagePath),
          let before=nativeUninstallIdentity(request.packagePath),before.owner==getuid(),
          !NSWorkspace.shared.runningApplications.contains(where:{
              $0.bundleURL?.resolvingSymlinksInPath().path == request.packagePath
          }) else {return false}
    guard nativeUninstallIdentity(request.packagePath)==before else {return false}
    do {
        var destination:NSURL?
        try FileManager.default.trashItem(at:URL(fileURLWithPath:request.packagePath),resultingItemURL:&destination)
        guard destination != nil else {return false}
        return writeNativeUninstallReply(["retired":true],closeOutput:false)
    } catch {return false}
}

enum NativeHelperMode: Equatable {
    case openLocal, registerLinks, confirmUninstall, trashPackage, uninstallError, retireMigration
}
func nativeHelperMode(_ arguments:[String]) -> NativeHelperMode? {
    switch arguments {
    case ["--open-local"]: return .openLocal
    case ["--register-links"]: return .registerLinks
    case ["--confirm-uninstall"]: return .confirmUninstall
    case ["--trash-package"]: return .trashPackage
    case ["--uninstall-error"]: return .uninstallError
    case ["--retire-migration-app"]: return .retireMigration
    default: return nil
    }
}

// Native launcher entry point. Parser qualification tests compile declarations
// above this marker and never register handlers or open a browser.
let arguments = Array(CommandLine.arguments.dropFirst())
if !arguments.isEmpty {
    let success:Bool
    switch nativeHelperMode(arguments) {
    case .openLocal: success=openLocalWorkspaceFromInput()
    case .registerLinks: success=registerLocalLinks()
    case .confirmUninstall: success=runNativeUninstall(confirmation:true)
    case .trashPackage: success=runNativeUninstall(confirmation:false)
    case .uninstallError: success=runNativeUninstallError()
    case .retireMigration: success=retireMigrationApplication()
    case nil: success=false
    }
    guard success else {
        let message=arguments==["--register-links"]
            ? "macOS could not select the installed companion for its links. Open the verified PhotoFindry installer again to repair the links; your library and saved work are kept.\n"
            : arguments==["--confirm-uninstall"] || arguments==["--trash-package"] || arguments==["--uninstall-error"]
                ? "PhotoFindry could not validate or uninstall its installed package. Nothing is permanently deleted.\n"
                : "PhotoFindry could not open the local workspace.\n"
        FileHandle.standardError.write(Data(message.utf8))
        exit(1)
    }
    exit(0)
} else {
    let app = NSApplication.shared
    let delegate = PhotoFindryLauncher()
    app.delegate = delegate
    app.run()
}
