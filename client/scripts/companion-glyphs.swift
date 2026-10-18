import Foundation
import AppKit

// Shares the native app's catalogue validation, installed-font shaping and cell
// rasterizer. Only three selected glyphs are sampled, never a font/bitmap atlas.
@main
struct CompanionGlyphs {
    @MainActor static func main() {
        guard CommandLine.arguments.count == 2,
              let catalog = try? EmojiCatalog(fileURL: URL(fileURLWithPath: CommandLine.arguments[1])),
              let line = readLine(), line.utf8.count <= 4096,
              let bytes = line.data(using: .utf8),
              let input = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
              let ids = input["ids"] as? [String], ids.count == 3,
              ids.allSatisfy({ $0.utf8.count <= 96 }) else { exit(2) }
        let cache = EmojiGlyphCache(catalog: catalog, capacity: 3)
        let beats: [[[String: Int]]] = ids.map { id in
            guard let entry = catalog.entry(id: id),
                  let image = cache.image(for: entry.glyph, pixels: 16),
                  let raster = EmojiPixelRaster(image: image) else { return [] }
            return raster.cells.map { ["column": $0.column, "row": $0.row,
                                      "red": Int($0.red), "green": Int($0.green), "blue": Int($0.blue)] }
        }
        guard let data = try? JSONSerialization.data(withJSONObject: ["side": 16, "beats": beats], options: [.sortedKeys]) else { exit(2) }
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([10]))
    }
}
