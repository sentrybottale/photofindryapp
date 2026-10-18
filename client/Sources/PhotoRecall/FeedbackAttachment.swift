import AppKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

// Shared unchanged by the native app and the local companion image helper.
struct FeedbackError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}
struct FeedbackAttachment: Identifiable, Sendable {
    static let maximumCount = 3
    static let maximumInputBytes = 20 * 1_024 * 1_024
    static let maximumOutputBytes = 5 * 1_024 * 1_024
    let id = UUID()
    let png: Data
    let width: Int
    let height: Int

    /// Only the file explicitly chosen by the user is read. Fresh sRGB pixels
    /// strip EXIF, GPS, comments and original filenames, not visible image content.
    static func load(_ url: URL) throws -> Self {
        let access = url.startAccessingSecurityScopedResource()
        defer { if access { url.stopAccessingSecurityScopedResource() } }
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard url.isFileURL, values.isRegularFile == true, let size = values.fileSize,
              size > 0, size <= maximumInputBytes else {
            throw FeedbackError("Choose a PNG or JPEG screenshot no larger than 20 MB.")
        }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let data = try handle.read(upToCount: maximumInputBytes + 1) ?? Data()
        guard data.count <= maximumInputBytes else { throw FeedbackError("That image is larger than 20 MB.") }
        return try decode(data)
    }

    static func decode(_ data: Data) throws -> Self {
        guard !data.isEmpty, data.count <= maximumInputBytes,
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              let type = CGImageSourceGetType(source) as String?, [UTType.png.identifier, UTType.jpeg.identifier].contains(type),
              CGImageSourceGetCount(source) == 1,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0, width <= 16_384, height <= 16_384,
              width * height <= 48_000_000 else {
            throw FeedbackError("Use a single PNG or JPEG image up to 48 megapixels.")
        }
        for maximumDimension in [2_560, 1_280] {
            guard let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumDimension
            ] as CFDictionary),
            let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
            let context = CGContext(data: nil, width: thumbnail.width, height: thumbnail.height,
                                    bitsPerComponent: 8, bytesPerRow: 0, space: colorSpace,
                                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
                throw FeedbackError("This screenshot could not be read. Try exporting it as a PNG.")
            }
            context.draw(thumbnail, in: CGRect(x: 0, y: 0, width: thumbnail.width, height: thumbnail.height))
            let output = NSMutableData()
            guard let image = context.makeImage(),
                  let destination = CGImageDestinationCreateWithData(output, UTType.png.identifier as CFString, 1, nil) else {
                throw FeedbackError("The screenshot copy could not be prepared.")
            }
            // ImageIO may describe the new bitmap's size/color space in EXIF;
            // no metadata from the original image is copied into this image.
            CGImageDestinationAddImage(destination, image, nil)
            guard CGImageDestinationFinalize(destination) else { throw FeedbackError("The screenshot copy could not be prepared.") }
            if output.length <= maximumOutputBytes {
                return Self(png: output as Data, width: image.width, height: image.height)
            }
        }
        throw FeedbackError("That screenshot is too large to submit. Choose a smaller crop.")
    }
}
