import Foundation

/// Public Unicode strings and names only. System font availability is a separate
/// question answered by EmojiGlyphCache on the current Mac.
struct EmojiCatalog {
    struct Entry: Decodable, Equatable {
        let id: String
        let glyph: String
        let name: String
    }

    private struct Document: Decodable {
        let schemaVersion: Int
        let emojiVersion: String
        let qualification: String
        let entries: [Entry]
    }

    enum LoadError: Error { case invalidCatalog }

    static let shared: EmojiCatalog = {
        guard let resources = Bundle.main.resourceURL,
              let catalog = try? EmojiCatalog(resourcesURL: resources)
        else { return EmojiCatalog() }
        return catalog
    }()

    let emojiVersion: String
    private let byID: [String: Entry]
    private let byGlyph: [String: Entry]
    var count: Int { byID.count }

    private init() {
        emojiVersion = ""
        byID = [:]
        byGlyph = [:]
    }

    init(resourcesURL: URL) throws {
        // Public Unicode data is packaged independently of the private engine.
        // Do not resolve a developer checkout when a bundle asset is missing.
        try self.init(fileURL: resourcesURL.appendingPathComponent("runtime/emoji/emoji-catalog.json"))
    }

    init(fileURL: URL) throws {
        let attributes = try FileManager.default.attributesOfItem(atPath: fileURL.path)
        guard let length = attributes[.size] as? NSNumber, length.intValue > 0,
              length.intValue <= 2 * 1024 * 1024 else { throw LoadError.invalidCatalog }
        let bytes = try Data(contentsOf: fileURL)
        guard bytes.count <= 2 * 1024 * 1024 else { throw LoadError.invalidCatalog }
        let document = try JSONDecoder().decode(Document.self, from: bytes)
        guard document.schemaVersion == 1, document.qualification == "fully-qualified",
              !document.emojiVersion.isEmpty, document.emojiVersion.utf8.count <= 16,
              !document.entries.isEmpty, document.entries.count <= 5_000 else { throw LoadError.invalidCatalog }
        var ids = [String: Entry](), glyphs = [String: Entry]()
        for entry in document.entries {
            guard !entry.name.isEmpty, entry.name.utf8.count <= 256,
                  entry.id.utf8.count <= 96, !entry.glyph.isEmpty, entry.glyph.utf16.count <= 32,
                  entry.id == Self.canonicalID(for: entry.glyph),
                  ids[entry.id] == nil, glyphs[entry.glyph] == nil else { throw LoadError.invalidCatalog }
            ids[entry.id] = entry
            glyphs[entry.glyph] = entry
        }
        emojiVersion = document.emojiVersion
        byID = ids
        byGlyph = glyphs
    }

    func entry(id: String) -> Entry? { byID[id] }
    func entry(glyph: String) -> Entry? { byGlyph[glyph] }
    func contains(id: String, glyph: String) -> Bool { byID[id]?.glyph == glyph }
    static func valid(id: String, glyph: String) -> Bool { shared.contains(id: id, glyph: glyph) }

    private static func canonicalID(for glyph: String) -> String {
        // Do not gate catalog loading on Swift's grapheme-table version. A
        // newer canonical sequence can remain catalogued on an older OS while
        // the actual font shaper rejects just that unsupported sequence.
        glyph.unicodeScalars.map { String(format: "%04X", $0.value) }.joined(separator: "-")
    }
}
