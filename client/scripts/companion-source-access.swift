// Fixed protocol1 access owner. Ordinary CLI updates retain this exact sealed
// bundle. Persistent bookmarks belong to this process; only fresh implicit
// bookmarks cross private pipes to publisher-authenticated installed readers.
import AppKit
import Foundation
import CryptoKit
import Security
import Darwin

let sourceAccessPublisherPin = "G363x/YCT07qbJyyI5minQjp3v+6SEw9sB7dU3pL6cM="
let sourceAccessBundleIdentifier = "com.photofindry.companion.source-access.v1"
let sourceAccessBundleName = "PhotoFindryAccess.app"
let sourceAccessInventoryPrefix = "bin/CompanionSourceAccess.app/"
struct AccessFailure: Error { let message: String; var status: String? = nil }
func accessRequire(_ condition: Bool, _ message: String) throws {
    if !condition { throw AccessFailure(message: message) }
}
func accessDigest(_ bytes: Data) -> String { SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined() }
func accessHex(_ value: String, count: Int) -> Bool { value.utf8.count == count && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) } }
func accessNumber(_ value: Any?) -> UInt64? {
    // NSNumber.doubleValue rounds large u64 generations. Parse its exact
    // integer representation, rejecting booleans, negatives and float syntax.
    guard let value = value as? NSNumber, CFGetTypeID(value) != CFBooleanGetTypeID() else { return nil }
    return UInt64(value.stringValue)
}
func accessDirectory(_ url: URL) throws {
    // Foundation standardization can rewrite a canonical /private/var path to
    // its /var system alias. Validate lexical components, then lstat every
    // actual ancestor instead of treating that rewrite as a permission grant.
    try accessRequire(url.isFileURL && url.path.hasPrefix("/") && !url.path.contains("//") && url.pathComponents.allSatisfy { $0 != "." && $0 != ".." } && url.path.utf8.count <= 16_384, "Invalid access-owner location.")
    var current = URL(fileURLWithPath: "/", isDirectory: true)
    for part in url.pathComponents.dropFirst() {
        current.appendPathComponent(part, isDirectory: true)
        var info = stat()
        let inspected = lstat(current.path, &info) == 0
        let sharedTemporary = inspected && info.st_uid == 0 && info.st_mode & S_ISVTX != 0
        let systemApplications = current.path == "/Applications" && info.st_uid == 0 && info.st_gid == 80 && info.st_mode & 0o002 == 0
        try accessRequire(inspected && (info.st_mode & S_IFMT) == S_IFDIR &&
                          [uid_t(0), geteuid()].contains(info.st_uid) && (info.st_mode & 0o022 == 0 || sharedTemporary || systemApplications),
                          "Unsafe access-owner directory.")
    }
}
func accessPrivateDirectory(_ url: URL) throws {
    try accessDirectory(url); var info = stat()
    try accessRequire(lstat(url.path, &info) == 0 && info.st_uid == geteuid() && info.st_mode & 0o077 == 0, "The access-owner state must be private to this account.")
}
func accessRead(_ url: URL, maximum: Int) throws -> Data {
    try accessDirectory(url.deletingLastPathComponent())
    let fd = open(url.path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
    if fd < 0 { throw AccessFailure(message: "Access-owner metadata is unavailable.", status: [EACCES, EPERM].contains(errno) ? "permission" : nil) }
    defer { close(fd) }
    var before = stat(), after = stat(), path = stat()
    try accessRequire(fstat(fd, &before) == 0 && (before.st_mode & S_IFMT) == S_IFREG && before.st_nlink == 1 &&
                      [uid_t(0), geteuid()].contains(before.st_uid) && before.st_mode & 0o022 == 0 &&
                      before.st_size >= 0 && before.st_size <= maximum, "Unsafe access-owner metadata.")
    var bytes = Data(), buffer = [UInt8](repeating: 0, count: 65_536)
    while bytes.count <= maximum {
        let n = Darwin.read(fd, &buffer, min(buffer.count, maximum + 1 - bytes.count))
        if n < 0 && errno == EINTR { continue }
        try accessRequire(n >= 0, "Cannot read access-owner metadata.")
        if n == 0 { break }; bytes.append(contentsOf: buffer.prefix(n))
    }
    try accessRequire(bytes.count <= maximum && fstat(fd, &after) == 0 && lstat(url.path, &path) == 0 &&
                      before.st_dev == after.st_dev && before.st_ino == after.st_ino && before.st_size == after.st_size &&
                      before.st_mtimespec.tv_sec == after.st_mtimespec.tv_sec && before.st_mtimespec.tv_nsec == after.st_mtimespec.tv_nsec &&
                      before.st_dev == path.st_dev && before.st_ino == path.st_ino, "Access-owner metadata changed while reading.")
    return bytes
}
func accessJSON(_ bytes: Data) throws -> [String: Any] {
    guard let value = try JSONSerialization.jsonObject(with: bytes) as? [String: Any] else { throw AccessFailure(message: "Invalid access-owner metadata.") }
    return value
}
func accessRelease(_ envelope: Data, pin: String = sourceAccessPublisherPin) throws -> [String: Any] {
    let value = try accessJSON(envelope)
    try accessRequire(Set(value.keys) == ["payload", "signature"], "Invalid access-owner release envelope.")
    guard let payload = value["payload"] as? String, let body = Data(base64Encoded: payload), body.count <= 65_536,
          let signature = value["signature"] as? String, let sig = Data(base64Encoded: signature), sig.count == 64,
          let key = Data(base64Encoded: pin), try Curve25519.Signing.PublicKey(rawRepresentation: key).isValidSignature(sig, for: body)
    else { throw AccessFailure(message: "The access-owner publisher signature is invalid.") }
    let release = try accessJSON(body)
    try accessRequire(accessNumber(release["schema"]) == 1 && release["kind"] as? String == "photofindry-cli-release" &&
                      release["platform"] as? String == "macos-arm64" && release["dataContract"] as? String == "photofindry-library-compatible-v1" &&
                      accessNumber(release["build"]).map { $0 > 0 } == true, "Unsupported access-owner publisher release.")
    return release
}
func accessInventory(_ raw: Data, release: [String: Any]) throws -> [String: String] {
    try accessRequire(release["inventorySHA256"] as? String == accessDigest(raw), "The access-owner inventory is not authenticated.")
    guard let text = String(data: raw, encoding: .utf8) else { throw AccessFailure(message: "Invalid access-owner inventory.") }
    var result: [String: String] = [:]
    for line in text.split(separator: "\n", omittingEmptySubsequences: false) where !line.isEmpty {
        let parts = line.components(separatedBy: "  ")
        try accessRequire(parts.count == 2 && accessHex(parts[0], count: 64) && parts[1].utf8.count <= 1024 &&
                          !parts[1].contains("\\") && parts[1].split(separator: "/", omittingEmptySubsequences: false).allSatisfy { !$0.isEmpty && $0 != "." && $0 != ".." } &&
                          result[parts[1]] == nil && result.count < 8192, "Invalid access-owner inventory row.")
        result[parts[1]] = parts[0]
    }
    return result
}
func accessOwnerRoot(_ bundle: URL = Bundle.main.bundleURL) throws -> URL {
    try accessRequire(bundle.lastPathComponent == sourceAccessBundleName && bundle.deletingLastPathComponent().lastPathComponent == "source-access", "The access owner is not installed at its retained location.")
    let root = bundle.deletingLastPathComponent().deletingLastPathComponent()
    try accessDirectory(bundle)
    try accessPrivateDirectory(root); try accessPrivateDirectory(bundle.deletingLastPathComponent())
    let release = try accessRelease(accessRead(root.appendingPathComponent("source-access/origin-release.json"), maximum: 131_072))
    let inventory = try accessInventory(accessRead(root.appendingPathComponent("source-access/origin-SHA256SUMS.txt"), maximum: 2 * 1024 * 1024), release: release)
    let expected = inventory.filter { $0.key.hasPrefix(sourceAccessInventoryPrefix) }
    try accessRequire(expected.count >= 3 && expected.count <= 32, "The retained access-owner bundle is incomplete.")
    var actual = Set<String>()
    guard let enumerator = FileManager.default.enumerator(at: bundle, includingPropertiesForKeys: nil) else { throw AccessFailure(message: "Cannot verify the retained access owner.") }
    for case let file as URL in enumerator {
        var info = stat(); try accessRequire(lstat(file.path, &info) == 0, "The retained access owner changed.")
        if (info.st_mode & S_IFMT) == S_IFDIR { try accessDirectory(file); continue }
        let suffix = String(file.path.dropFirst(bundle.path.count + 1)), name = sourceAccessInventoryPrefix + suffix
        try accessRequire(expected[name] == accessDigest(accessRead(file, maximum: 64 * 1024 * 1024)), "The retained access owner does not match its publisher inventory.")
        actual.insert(name)
    }
    try accessRequire(actual == Set(expected.keys), "The retained access-owner bundle is incomplete.")
    let info = try PropertyListSerialization.propertyList(from: accessRead(bundle.appendingPathComponent("Contents/Info.plist"), maximum: 16_384), format: nil) as? [String: Any]
    try accessRequire(info?["CFBundleIdentifier"] as? String == sourceAccessBundleIdentifier && info?["CFBundleVersion"] as? String == "1" && info?["CFBundleURLTypes"] == nil, "The retained access-owner identity changed.")
    return root
}
func accessOwnerLease(root: URL) throws -> Int32 {
    let path = root.appendingPathComponent("source-access/owner.lock")
    let fd = open(path.path, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0o600)
    guard fd >= 0 else { throw AccessFailure(message: "Cannot lease the retained source-access owner.") }
    var info = stat()
    guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFREG, info.st_nlink == 1, info.st_uid == geteuid(), info.st_mode & 0o077 == 0,
          flock(fd, LOCK_SH | LOCK_NB) == 0 else { close(fd); throw AccessFailure(message: "The retained source-access owner is being removed.") }
    return fd
}
func accessCaller(root: URL, operation: String) throws {
    var input = stat(), output = stat()
    try accessRequire(fstat(STDIN_FILENO, &input) == 0 && fstat(STDOUT_FILENO, &output) == 0 &&
                      input.st_mode & S_IFMT == S_IFIFO && output.st_mode & S_IFMT == S_IFIFO,
                      "Access grants require private caller pipes.")
    let parent = getppid(); try accessRequire(parent > 1, "The access-owner caller is unavailable.")
    var dynamic: SecCode?, signing: CFDictionary?
    let flags = SecCSFlags(rawValue: 0)
    try accessRequire(SecCodeCopyGuestWithAttributes(nil, [kSecGuestAttributePid as String: NSNumber(value: parent)] as CFDictionary, flags, &dynamic) == errSecSuccess && dynamic != nil, "Cannot authenticate the access-owner caller.")
    // Security explicitly accepts either a dynamic Code or StaticCode here.
    // Swift imports distinct opaque CF types; preserve the dynamic object so
    // kSecCodeInfoStatus comes from the running caller, not only its disk file.
    let running = unsafeBitCast(dynamic!, to: SecStaticCode.self)
    try accessRequire(SecCodeCheckValidity(dynamic!, flags, nil) == errSecSuccess &&
                      SecCodeCopySigningInformation(running, SecCSFlags(rawValue: kSecCSSigningInformation | kSecCSDynamicInformation), &signing) == errSecSuccess,
                      "The access-owner caller code is invalid.")
    try accessHardenedCaller(signing as? [String: Any] ?? [:])
    guard let main = (signing as? [String: Any])?[kSecCodeInfoMainExecutable as String] as? URL else { throw AccessFailure(message: "Cannot identify the access-owner caller.") }
    let executable = URL(fileURLWithPath: accessLocation(main))
    let state = try accessJSON(accessRead(root.appendingPathComponent("installation.json"), maximum: 32_768))
    guard let build = accessNumber(state["current"]), build > 0 else { throw AccessFailure(message: "The access-owner installation is unavailable.") }
    let version = root.appendingPathComponent("versions/\(build)", isDirectory: true), package = version.appendingPathComponent("package", isDirectory: true)
    let relative: String
    if executable.path == package.appendingPathComponent("bin/photofindry").path { relative = "bin/photofindry" }
    else if operation == "restore" && executable.path == package.appendingPathComponent("runtime/PhotoWorker").path { relative = "runtime/PhotoWorker" }
    else { throw AccessFailure(message: "Only the active authenticated PhotoFindry reader may restore source access.") }
    let release = try accessRelease(accessRead(version.appendingPathComponent("release.json"), maximum: 131_072))
    try accessRequire(accessNumber(release["build"]) == build, "The access-owner caller release changed.")
    let inventory = try accessInventory(accessRead(package.appendingPathComponent("SHA256SUMS.txt"), maximum: 2 * 1024 * 1024), release: release)
    try accessRequire(inventory[relative] == accessDigest(accessRead(executable, maximum: 128 * 1024 * 1024)) && getppid() == parent && SecCodeCheckValidity(dynamic!, flags, nil) == errSecSuccess, "The access-owner caller does not match its publisher-authenticated code.")
}
func accessHardenedCaller(_ signing: [String: Any]) throws {
    let runtime = UInt64(SecCodeSignatureFlags.runtime.rawValue)
    let library = UInt64(SecCodeSignatureFlags.libraryValidation.rawValue)
    let flags = (signing[kSecCodeInfoFlags as String] as? NSNumber)?.uint64Value ?? 0
    let status = (signing[kSecCodeInfoStatus as String] as? NSNumber)?.uint64Value ?? 0
    let entitlements = signing[kSecCodeInfoEntitlementsDict as String] as? [String: Any] ?? [:]
    try accessRequire(signing[kSecCodeInfoEntitlements as String] == nil || signing[kSecCodeInfoEntitlementsDict as String] is [String: Any], "The source-access caller entitlements could not be validated.")
    let forbidden = ["com.apple.security.get-task-allow", "com.apple.security.cs.debugger", "com.apple.security.cs.allow-dyld-environment-variables", "com.apple.security.cs.disable-library-validation", "com.apple.security.cs.allow-unsigned-executable-memory", "com.apple.security.cs.allow-jit", "com.apple.security.cs.disable-executable-page-protection"]
    try accessRequire(flags & (runtime | library) == runtime | library && status & (runtime | library) == runtime | library && status & UInt64(SecCodeStatus.valid.rawValue) != 0 && status & UInt64(SecCodeStatus.debugged.rawValue) == 0 &&
                      forbidden.allSatisfy { key in guard let value = entitlements[key] else { return true }; return (value as? NSNumber)?.boolValue == false },
                      "The source-access caller requires an intact hardened runtime without debugging or library-validation exceptions.")
}
func accessHardenedSelf() throws {
    var code: SecCode?, info: CFDictionary?
    try accessRequire(SecCodeCopySelf(SecCSFlags(rawValue: 0), &code) == errSecSuccess && code != nil &&
                      SecCodeCheckValidity(code!, SecCSFlags(rawValue: 0), nil) == errSecSuccess &&
                      SecCodeCopySigningInformation(unsafeBitCast(code!, to: SecStaticCode.self), SecCSFlags(rawValue: kSecCSSigningInformation | kSecCSDynamicInformation), &info) == errSecSuccess,
                      "The retained source-access helper code is invalid.")
    try accessHardenedCaller(info as? [String: Any] ?? [:])
}
func accessStore(root: URL, create: Bool = true) throws -> URL {
    let store = root.appendingPathComponent("source-access-state", isDirectory: true)
    if create && !FileManager.default.fileExists(atPath: store.path) { try FileManager.default.createDirectory(at: store, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700]) }
    try accessPrivateDirectory(store)
    return store
}
func accessSave(_ record: [String: Any], root: URL, id: String) throws {
    try accessRequire(accessHex(id, count: 64), "Invalid source-access reference.")
    let store = try accessStore(root: root), destination = store.appendingPathComponent(id + ".json")
    let data = try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
    try accessRequire(data.count <= 131_072, "The source-access bookmark exceeds its bound.")
    let temporary = store.appendingPathComponent(".stage-" + UUID().uuidString)
    let fd = open(temporary.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
    try accessRequire(fd >= 0, "Cannot retain the selected source access.")
    defer { close(fd); try? FileManager.default.removeItem(at: temporary) }
    var written = 0
    try data.withUnsafeBytes { bytes in
        while written < data.count { let n = Darwin.write(fd, bytes.baseAddress!.advanced(by: written), data.count - written); if n < 0 && errno == EINTR { continue }; try accessRequire(n > 0, "Cannot retain the selected source access."); written += n }
    }
    try accessRequire(fsync(fd) == 0 && rename(temporary.path, destination.path) == 0, "Cannot publish the selected source access.")
    let directory = open(store.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC); if directory >= 0 { _ = fsync(directory); close(directory) }
}
func accessRandom() throws -> String {
    var bytes = [UInt8](repeating: 0, count: 32)
    try accessRequire(SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess, "Cannot identify the selected source access.")
    return bytes.map { String(format: "%02x", $0) }.joined()
}
func accessLocation(_ url: URL) -> String {
    guard let path = realpath(url.path, nil) else { return url.path }
    defer { free(path) }; return String(cString: path)
}
func accessTransfer(_ url: URL, id: String, selectedPath: String) throws -> [String: Any] {
    let data = try url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
    try accessRequire(!data.isEmpty && data.count <= 65_536, "The source access could not be transferred within its bound.")
    return ["ownerGrantId": id, "selectedPath": selectedPath, "path": accessLocation(url), "readerBookmark": data.base64EncodedString(), "status": "available"]
}
func accessStoredGrant(root: URL, id: String) throws -> [String: Any] {
    let file = root.appendingPathComponent("source-access-state/\(id).json")
    var info = stat()
    if lstat(file.path, &info) != 0 {
        let status = errno == ENOENT ? "missing" : [EACCES, EPERM].contains(errno) ? "permission" : "unavailable"
        throw AccessFailure(message: "The retained source-access reference is unavailable.", status: status)
    }
    try accessDirectory(file.deletingLastPathComponent())
    guard info.st_mode & S_IFMT == S_IFREG, info.st_nlink == 1, info.st_uid == geteuid(), info.st_mode & 0o077 == 0 else { throw AccessFailure(message: "Invalid retained source access.", status: "invalid") }
    let bytes = try accessRead(file, maximum: 131_072)
    do { return try accessJSON(bytes) } catch { throw AccessFailure(message: "Invalid retained source access.", status: "invalid") }
}
func accessFailureStatus(_ error: Error, depth: Int = 0) -> String {
    if depth >= 4 { return "unavailable" }
    if let status = (error as? AccessFailure)?.status { return status }
    let value = error as NSError
    if value.domain == NSPOSIXErrorDomain && [Int(EACCES), Int(EPERM)].contains(value.code) { return "permission" }
    if value.domain == NSCocoaErrorDomain {
        if [NSFileReadNoPermissionError, NSFileWriteNoPermissionError].contains(value.code) { return "permission" }
        if value.code == NSFileReadNoSuchFileError { return "missing" }
        if [NSFileReadCorruptFileError, NSFileReadUnsupportedSchemeError].contains(value.code) { return "invalid" }
    }
    if let underlying = value.userInfo[NSUnderlyingErrorKey] as? NSError, underlying !== value { return accessFailureStatus(underlying, depth: depth + 1) }
    return "unavailable"
}
func accessRequest(_ value: [String: Any], root: URL, select: () throws -> URL?) throws -> [String: Any] {
    guard let operation = value["operation"] as? String, ["select", "restore"].contains(operation),
          accessNumber(value["schema"]) == 1, let nonce = value["requestNonce"] as? String, accessHex(nonce, count: 64), let generation = accessNumber(value["generation"])
    else { throw AccessFailure(message: "Unsupported source-access request.") }
    var response: [String: Any] = ["schema": 1, "operation": operation, "requestNonce": nonce, "generation": generation]
    if operation == "select" {
        try accessRequire(Set(value.keys) == ["schema", "operation", "requestNonce", "generation", "kind"] && ["folder", "sourceFolder"].contains(value["kind"] as? String ?? ""), "Unsupported source selection.")
        guard let url = try select() else { response["cancelled"] = true; return response }
        let held = url.startAccessingSecurityScopedResource(); defer { if held { url.stopAccessingSecurityScopedResource() } }
        let bookmark = try url.bookmarkData(options: [.withSecurityScope, .securityScopeAllowOnlyReadAccess], includingResourceValuesForKeys: nil, relativeTo: nil)
        try accessRequire(!bookmark.isEmpty && bookmark.count <= 65_536, "macOS did not create a persistent source-access grant.")
        let id = try accessRandom(), selected = accessLocation(url)
        // The panel URL may carry broader scope. Mint the reader transfer from
        // the re-resolved read-only owner bookmark, exactly as on later restore.
        var stale = false
        let readOnly = try URL(resolvingBookmarkData: bookmark, options: [.withSecurityScope, .withoutUI, .withoutMounting], relativeTo: nil, bookmarkDataIsStale: &stale)
        try accessRequire(readOnly.isFileURL && !stale && readOnly.startAccessingSecurityScopedResource(), "macOS did not activate the selected read-only source access.")
        defer { readOnly.stopAccessingSecurityScopedResource() }
        let transfer = try accessTransfer(readOnly, id: id, selectedPath: selected)
        var record: [String: Any] = ["schema": 1, "ownerGrantId": id, "selectedPath": selected, "bookmark": bookmark.base64EncodedString()]
        let volume = try? readOnly.resourceValues(forKeys: [.volumeUUIDStringKey, .volumeIsRemovableKey, .volumeIsEjectableKey])
        if let uuid = volume?.volumeUUIDString { record["volumeUUID"] = uuid }
        record["checkMountedVolume"] = volume?.volumeIsRemovable == true || volume?.volumeIsEjectable == true
        try accessSave(record, root: root, id: id)
        response["cancelled"] = false; response["selection"] = transfer
    } else {
        try accessRequire(Set(value.keys) == ["schema", "operation", "requestNonce", "generation", "grants"], "Unsupported source-access restore request.")
        guard let grants = value["grants"] as? [[String: Any]], !grants.isEmpty && grants.count <= 16 else { throw AccessFailure(message: "Restore one to sixteen registered source references.") }
        var entries: [[String: Any]] = [], seen = Set<String>()
        var mountedUUIDs: Set<String>?
        for entry in grants {
            guard Set(entry.keys) == ["ownerGrantId"], let id = entry["ownerGrantId"] as? String, accessHex(id, count: 64), seen.insert(id).inserted else { throw AccessFailure(message: "Invalid source-access reference.") }
            do {
                let record = try accessStoredGrant(root: root, id: id)
                guard accessNumber(record["schema"]) == 1, record["ownerGrantId"] as? String == id, let selected = record["selectedPath"] as? String,
                      selected.hasPrefix("/"), selected.utf8.count <= 16_384, let encoded = record["bookmark"] as? String, let data = Data(base64Encoded: encoded), !data.isEmpty, data.count <= 65_536 else { throw AccessFailure(message: "Invalid retained source access.", status: "invalid") }
                if record["checkMountedVolume"] as? Bool == true, let uuid = record["volumeUUID"] as? String, !uuid.isEmpty {
                    if mountedUUIDs == nil { let volumes = FileManager.default.mountedVolumeURLs(includingResourceValuesForKeys: [.volumeUUIDStringKey], options: []) ?? []; mountedUUIDs = Set(volumes.prefix(256).compactMap { try? $0.resourceValues(forKeys: [.volumeUUIDStringKey]).volumeUUIDString }.compactMap { $0 }) }
                    if mountedUUIDs?.contains(uuid) == false { throw AccessFailure(message: "The selected source volume is not mounted.", status: "offline") }
                }
                var stale = false
                let url = try URL(resolvingBookmarkData: data, options: [.withSecurityScope, .withoutUI, .withoutMounting], relativeTo: nil, bookmarkDataIsStale: &stale)
                // A present folder on a different volume than the one chosen, or one
                // macOS will not reactivate, needs one new folder choice rather than
                // silent retries (for example after a network drive remounts).
                let present = (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
                if present, let saved = record["volumeUUID"] as? String, !saved.isEmpty,
                   let current = try? url.resourceValues(forKeys: [.volumeUUIDStringKey]).volumeUUIDString, current != saved {
                    throw AccessFailure(message: "The selected folder is now on a different volume.", status: "invalid")
                }
                guard url.isFileURL && url.startAccessingSecurityScopedResource() else {
                    throw AccessFailure(message: "macOS did not restore the selected source access.", status: present ? "invalid" : "unavailable")
                }
                defer { url.stopAccessingSecurityScopedResource() }
                let transfer = try accessTransfer(url, id: id, selectedPath: selected)
                if stale { let refreshed = try url.bookmarkData(options: [.withSecurityScope, .securityScopeAllowOnlyReadAccess], includingResourceValuesForKeys: nil, relativeTo: nil); try accessRequire(refreshed.count <= 65_536, "The refreshed source access exceeds its bound."); var next = record; next["bookmark"] = refreshed.base64EncodedString(); try accessSave(next, root: root, id: id) }
                entries.append(transfer)
            } catch { entries.append(["ownerGrantId": id, "status": accessFailureStatus(error), "message": "The selected folder access could not be restored. Your library and saved work are kept."]) }
        }
        response["entries"] = entries
    }
    return response
}

#if !SOURCE_ACCESS_TESTS
@main
struct SourceAccessMain {
static func main() {
do {
    try accessHardenedSelf()
    let root = try accessOwnerRoot()
    let ownerLease = try accessOwnerLease(root: root); defer { close(ownerLease) }
    _ = try accessOwnerRoot() // Fence uninstall between initial verification and lease.
    let owner = try accessJSON(accessRead(root.appendingPathComponent("source-access/owner.json"), maximum: 4096))
    try accessRequire(accessNumber(owner["schema"]) == 1 && accessNumber(owner["protocol"]) == 1 && owner["removed"] as? Bool == false, "The retained source-access owner is disabled.")
    // Authenticate before reading any request or persisted grant. No command
    // line operation or environment variable can disable this admission.
    try accessCaller(root: root, operation: "restore")
    var raw = Data()
    while raw.count <= 32_768 { guard let chunk = try FileHandle.standardInput.read(upToCount: 32_769 - raw.count), !chunk.isEmpty else { break }; raw.append(chunk) }
    try accessRequire(raw.count <= 32_768, "The source-access request exceeds its bound.")
    let request = try accessJSON(raw)
    if request["operation"] as? String == "select" { try accessCaller(root: root, operation: "select") }
    let response = try accessRequest(request, root: root) {
        let app = NSApplication.shared; app.setActivationPolicy(.accessory); app.finishLaunching(); app.activate(ignoringOtherApps: true)
        let panel = NSOpenPanel(); panel.title = "Choose originals folder"; panel.message = "PhotoFindry keeps read-only access to the folder you choose."; panel.canChooseDirectories = true; panel.canChooseFiles = false; panel.allowsMultipleSelection = false
        return panel.runModal() == .OK ? panel.url : nil
    }
    let output = try JSONSerialization.data(withJSONObject: response, options: [.sortedKeys])
    try accessRequire(output.count <= 2 * 1024 * 1024, "The source-access response exceeds its bound.")
    FileHandle.standardOutput.write(output); FileHandle.standardOutput.write(Data([10]))
} catch {
    // Fixed local reasons only. Never print bookmark data, selected paths or
    // framework error descriptions to a log or a terminal.
    let message = (error as? AccessFailure)?.message ?? "The native source-access operation failed. Your library and saved work are kept."
    if let output = try? JSONSerialization.data(withJSONObject: ["error": message], options: [.sortedKeys]) { FileHandle.standardOutput.write(output); FileHandle.standardOutput.write(Data([10])) }
    exit(2)
}
}
}
#endif
