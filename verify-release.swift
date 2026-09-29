// PhotoFindry standalone release-verification utility. No application code.
// Requires Apple's Command Line Tools (Swift and CryptoKit). Performs no network requests.
import Foundation
import CryptoKit
import Darwin

let pinnedPublicKey = "G363x/YCT07qbJyyI5minQjp3v+6SEw9sB7dU3pL6cM="

enum VerificationError: Error, CustomStringConvertible {
    case rejected(String)
    var description: String { switch self { case let .rejected(message): return message } }
}
func reject(_ message: String) throws -> Never { throw VerificationError.rejected(message) }
func regularFile(_ path: URL, maximum: Int64) throws -> FileHandle {
    let descriptor = path.withUnsafeFileSystemRepresentation { open($0!, O_RDONLY | O_NOFOLLOW | O_NONBLOCK) }
    guard descriptor >= 0 else { try reject("Cannot open regular file: \(path.lastPathComponent)") }
    var metadata = stat()
    guard fstat(descriptor, &metadata) == 0,
          (metadata.st_mode & mode_t(S_IFMT)) == mode_t(S_IFREG),
          metadata.st_size >= 0, metadata.st_size <= maximum else {
        close(descriptor)
        try reject("File is unsafe or outside the size limit: \(path.lastPathComponent)")
    }
    return FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
}
func boundedData(_ path: URL, maximum: Int64) throws -> Data {
    let file = try regularFile(path, maximum: maximum)
    defer { try? file.close() }
    let data = try file.read(upToCount: Int(maximum) + 1) ?? Data()
    guard data.count <= maximum else { try reject("File grew beyond its size limit.") }
    return data
}
func hash(_ path: URL) throws -> String {
    let file = try regularFile(path, maximum: 2 * 1024 * 1024 * 1024)
    defer { try? file.close() }
    var value = SHA256(), count: Int64 = 0
    while let chunk = try file.read(upToCount: 1024 * 1024), !chunk.isEmpty {
        count += Int64(chunk.count)
        guard count <= 2 * 1024 * 1024 * 1024 else { try reject("Artifact exceeds the size limit.") }
        value.update(data: chunk)
    }
    return value.finalize().map { String(format: "%02x", $0) }.joined()
}

do {
    guard CommandLine.arguments.count <= 2 else { try reject("Usage: swift verify-release.swift [release-directory]") }
    let directory = URL(fileURLWithPath: CommandLine.arguments.count == 2 ? CommandLine.arguments[1] : FileManager.default.currentDirectoryPath, isDirectory: true)
    let manifest = try boundedData(directory.appendingPathComponent("SHA256SUMS.txt"), maximum: 128 * 1024)
    let signatureData = try boundedData(directory.appendingPathComponent("SHA256SUMS.ed25519"), maximum: 256)
    guard let signatureText = String(data: signatureData, encoding: .utf8),
          let signature = Data(base64Encoded: signatureText.trimmingCharacters(in: .whitespacesAndNewlines)), signature.count == 64,
          let publicBytes = Data(base64Encoded: pinnedPublicKey), publicBytes.count == 32 else {
        try reject("Invalid signature or public key encoding.")
    }
    let publicKey = try Curve25519.Signing.PublicKey(rawRepresentation: publicBytes)
    guard publicKey.isValidSignature(signature, for: manifest) else { try reject("Release inventory signature is INVALID. Do not install these files.") }
    guard let contents = String(data: manifest, encoding: .utf8), contents.hasSuffix("\n") else { try reject("Invalid signed inventory encoding.") }
    let lines = contents.dropLast().split(separator: "\n", omittingEmptySubsequences: false)
    guard !lines.isEmpty, lines.count <= 100 else { try reject("Invalid inventory size.") }
    var names = Set<String>()
    for rawLine in lines {
        let line = String(rawLine)
        guard line.utf8.count >= 67, line.dropFirst(64).hasPrefix("  ") else { try reject("Malformed inventory entry.") }
        let expected = String(line.prefix(64)), name = String(line.dropFirst(66))
        guard expected.allSatisfy({ "0123456789abcdef".contains($0) }),
              !name.isEmpty, name.utf8.count <= 255, !name.hasPrefix("."),
              name.unicodeScalars.allSatisfy({ $0.isASCII && CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-").contains($0) }),
              names.insert(name).inserted else { try reject("Unsafe or duplicate inventory entry.") }
        guard try hash(directory.appendingPathComponent(name)) == expected else { try reject("Checksum mismatch: \(name). Do not install this file.") }
        print("Verified: \(name)")
    }
    print("Release inventory signature and all \(names.count) listed artifacts verified against the pinned PhotoFindry public key.")
} catch {
    fputs("Verification failed: \(error)\n", stderr)
    exit(1)
}
