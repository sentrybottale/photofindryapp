import AppKit
import CoreText

/// Small, in-memory sprites rendered from the installed system font. No font
/// bytes or generated artwork leave this process or become app-bundle assets.
@MainActor
final class EmojiGlyphCache {
    static let shared = EmojiGlyphCache()

    private struct ImageKey: Hashable {
        let glyph: String
        let pixels: Int
    }

    private enum Shape {
        case unsupported
        case supported(CTLine)
    }

    private struct Cache<Key: Hashable, Value> {
        private struct Slot { let value: Value; var access: UInt64 }
        let capacity: Int
        private var clock: UInt64 = 0
        private var values = [Key: Slot]()
        var count: Int { values.count }
        init(capacity: Int) { self.capacity = capacity }

        mutating func get(_ key: Key) -> Value? {
            guard var slot = values[key] else { return nil }
            clock &+= 1
            slot.access = clock
            values[key] = slot
            return slot.value
        }

        mutating func insert(_ value: Value, for key: Key) {
            if values[key] == nil, values.count >= capacity,
               let oldest = values.min(by: { $0.value.access < $1.value.access })?.key {
                values.removeValue(forKey: oldest)
            }
            clock &+= 1
            values[key] = Slot(value: value, access: clock)
        }
    }

    private let catalog: EmojiCatalog
    private let font = CTFontCreateWithName("AppleColorEmoji" as CFString, 128, nil)
    private var shapes: Cache<String, Shape>
    private var sources: Cache<String, CGImage>
    private var images: Cache<ImageKey, CGImage>
    private(set) var shapeCount = 0
    private(set) var renderCount = 0
    var cachedShapeCount: Int { shapes.count }
    var cachedSourceCount: Int { sources.count }
    var cachedImageCount: Int { images.count }

    /// Capacity is clamped so accidental callers cannot make a full-font atlas.
    /// At the default limit, high-resolution source pixels occupy at most 8 MiB;
    /// small sprites occupy at most 1 MiB. Support checks do not rasterize glyphs.
    init(catalog: EmojiCatalog = .shared, capacity: Int = 128) {
        self.catalog = catalog
        let bounded = min(128, max(1, capacity))
        shapes = Cache(capacity: bounded * 2)
        sources = Cache(capacity: bounded)
        images = Cache(capacity: bounded * 2)
    }

    func supports(_ glyph: String) -> Bool {
        if case .supported = shape(for: glyph) { return true }
        return false
    }

    /// The LCD reads this as a cell matrix; it must not stretch the image over a grid.
    func image(for glyph: String, pixels: Int = 16) -> CGImage? {
        guard [12, 16, 20, 24, 32].contains(pixels) else { return nil }
        let key = ImageKey(glyph: glyph, pixels: pixels)
        if let cached = images.get(key) { return cached }
        let source: CGImage
        if let cached = sources.get(glyph) {
            source = cached
        } else {
            guard case let .supported(line) = shape(for: glyph),
                  let rendered = renderSource(line) else { return nil }
            source = rendered
            sources.insert(source, for: glyph)
        }
        guard let context = Self.context(pixels: pixels) else { return nil }
        // One transparent cell permits a whole-cell idle bob without clipping.
        // Center on actual ink bounds rather than the font's baseline.
        let available = CGFloat(pixels - 2)
        let scale = available / CGFloat(max(source.width, source.height))
        let size = CGSize(width: CGFloat(source.width) * scale, height: CGFloat(source.height) * scale)
        let rect = CGRect(x: (CGFloat(pixels) - size.width) / 2,
                          y: (CGFloat(pixels) - size.height) / 2, width: size.width, height: size.height)
        context.interpolationQuality = .high
        context.draw(source, in: rect)
        guard let image = context.makeImage() else { return nil }
        images.insert(image, for: key)
        return image
    }

    private func shape(for glyph: String) -> Shape {
        guard glyph.utf16.count <= 32, catalog.entry(glyph: glyph) != nil else { return .unsupported }
        if let cached = shapes.get(glyph) { return cached }
        shapeCount += 1
        let line = makeLine(glyph)
        let result: Shape
        if let glyphID = singleEmojiGlyph(in: line, utf16Length: glyph.utf16.count),
           !ignoresTagSequence(glyph, glyphID: glyphID) {
            result = .supported(line)
        } else {
            result = .unsupported
        }
        shapes.insert(result, for: glyph)
        return result
    }

    private func makeLine(_ glyph: String) -> CTLine {
        CTLineCreateWithAttributedString(NSAttributedString(string: glyph, attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
        ]))
    }

    private func singleEmojiGlyph(in line: CTLine, utf16Length: Int) -> CGGlyph? {
        let runs = CTLineGetGlyphRuns(line) as! [CTRun]
        // The entire grapheme must shape into one actual Apple emoji glyph.
        // Scalar support alone would incorrectly accept decomposed ZWJ stories,
        // flags and skin tones; LastResort/tofu and text fallbacks fail here.
        guard runs.count == 1, let run = runs.first, CTRunGetGlyphCount(run) == 1 else { return nil }
        let attributes = CTRunGetAttributes(run) as NSDictionary
        guard let runFont = attributes[kCTFontAttributeName] else { return nil }
        let shapedFont = runFont as! CTFont
        guard CTFontCopyPostScriptName(shapedFont) as String == "AppleColorEmoji" else { return nil }
        let range = CTRunGetStringRange(run)
        guard range.location == 0, range.length == utf16Length else { return nil }
        var glyph: CGGlyph = 0
        var advance = CGSize.zero
        CTRunGetGlyphs(run, CFRange(location: 0, length: 1), &glyph)
        CTRunGetAdvances(run, CFRange(location: 0, length: 1), &advance)
        guard glyph != 0, advance.width > 0, advance.width <= 160 else { return nil }
        return glyph
    }

    private func ignoresTagSequence(_ glyph: String, glyphID: CGGlyph) -> Bool {
        guard glyph.unicodeScalars.contains(where: { (0xE0020...0xE007F).contains($0.value) }),
              let first = glyph.unicodeScalars.first else { return false }
        // Older fonts can silently drop subdivision tags and draw a plain black
        // flag as a single glyph. That is not support for the requested flag.
        let base = String(first)
        return singleEmojiGlyph(in: makeLine(base), utf16Length: base.utf16.count) == glyphID
    }

    private func renderSource(_ line: CTLine) -> CGImage? {
        let pixels = 192
        guard let context = Self.context(pixels: pixels) else { return nil }
        context.textMatrix = .identity
        context.textPosition = CGPoint(x: 32, y: 48)
        CTLineDraw(line, context)
        guard let image = context.makeImage(), let data = context.data else { return nil }
        let bytes = data.assumingMemoryBound(to: UInt8.self)
        var minX = pixels, minY = pixels, maxX = -1, maxY = -1
        for y in 0..<pixels {
            for x in 0..<pixels where bytes[y * context.bytesPerRow + x * 4 + 3] > 0 {
                minX = min(minX, x); minY = min(minY, y)
                maxX = max(maxX, x); maxY = max(maxY, y)
            }
        }
        guard maxX >= minX, maxY >= minY else { return nil }
        // If font behavior changes, reject overflowing artwork instead of
        // quietly clipping it. The ordinary 128-point glyph has 32px padding.
        guard minX > 0, minY > 0, maxX < pixels - 1, maxY < pixels - 1,
              maxX - minX + 1 <= 128, maxY - minY + 1 <= 128 else { return nil }
        guard let cropped = image.cropping(to: CGRect(x: minX, y: minY,
                            width: maxX - minX + 1, height: maxY - minY + 1)) else { return nil }
        // CGImage cropping can retain the original 192px backing allocation.
        // Copy just the ink into a fresh bounded buffer before caching it.
        guard let compact = CGContext(data: nil, width: cropped.width, height: cropped.height,
                bitsPerComponent: 8, bytesPerRow: cropped.width * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo:
                    CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        compact.interpolationQuality = .none
        compact.draw(cropped, in: CGRect(x: 0, y: 0, width: cropped.width, height: cropped.height))
        guard let retained = compact.makeImage() else { return nil }
        renderCount += 1
        return retained
    }

    private static func context(pixels: Int) -> CGContext? {
        CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: pixels * 4,
                  space: CGColorSpaceCreateDeviceRGB(), bitmapInfo:
                    CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue)
    }
}
