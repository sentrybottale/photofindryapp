import Foundation
import CryptoKit

// A local, fixed decoder shared with the application. Only a previously selected
// file path arrives on stdin; no executable instructions or output path exists.
@main
struct CompanionFeedbackImage {
    static func finish(_ value: [String: Any], status: Int32 = 0) -> Never {
        if let bytes = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
            FileHandle.standardOutput.write(bytes)
            FileHandle.standardOutput.write(Data([10]))
        }
        exit(status)
    }
    static func main() {
        do {
            var bytes = Data()
            while let chunk = try FileHandle.standardInput.read(upToCount: min(8_192, 131_073 - bytes.count)), !chunk.isEmpty {
                bytes.append(chunk)
                if bytes.count > 131_072 { break }
            }
            guard CommandLine.arguments.count == 1, bytes.count <= 131_072,
                  let input = try JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  input.count == 1, let path = input["path"] as? String,
                  path.hasPrefix("/"), path.utf8.count <= 16_384, !path.contains("\0") else {
                throw FeedbackError("Choose a screenshot using the native file chooser.")
            }
            let attachment = try FeedbackAttachment.load(URL(fileURLWithPath: path))
            finish(["pngBase64": attachment.png.base64EncodedString(), "width": attachment.width,
                    "height": attachment.height, "bytes": attachment.png.count,
                    "sha256": SHA256.hash(data: attachment.png).map { String(format: "%02x", $0) }.joined()])
        } catch let error as FeedbackError {
            finish(["error": error.message], status: 2)
        } catch {
            finish(["error": "The screenshot could not be read. Choose another PNG or JPEG."], status: 2)
        }
    }
}
