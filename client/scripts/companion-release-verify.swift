// Verify release metadata using the existing publisher pin. No Keychain access,
// network, file selection, shell execution or downloaded trust roots.
import Foundation
import CryptoKit
let pin = "G363x/YCT07qbJyyI5minQjp3v+6SEw9sB7dU3pL6cM="
var input = Data()
do {
    while input.count <= 131072 {
        guard let chunk = try FileHandle.standardInput.read(upToCount: 131073-input.count), !chunk.isEmpty else { break }
        input.append(chunk)
    }
    guard input.count <= 131072,
          let value = try JSONSerialization.jsonObject(with: input) as? [String:Any],
          Set(value.keys) == Set(["payload", "signature"]),
          let encoded = value["payload"] as? String, let body = Data(base64Encoded: encoded), body.count <= 65536,
          let text = value["signature"] as? String, let signature = Data(base64Encoded: text), signature.count == 64,
          let bytes = Data(base64Encoded: pin) else { exit(2) }
    let key = try Curve25519.Signing.PublicKey(rawRepresentation: bytes)
    guard key.isValidSignature(signature, for: body) else { exit(3) }
    print("{\"valid\":true}")
} catch { exit(2) }
