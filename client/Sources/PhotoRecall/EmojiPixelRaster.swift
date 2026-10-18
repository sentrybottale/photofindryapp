import CoreGraphics
import SwiftUI

/// A font glyph sampled onto the fixed LCD, not an image stretched over it.
/// Every occupied cell has one solid colour and the same visible gap.
struct EmojiPixelRaster: Equatable, Sendable {
    struct Cell: Equatable, Sendable {
        let column: Int
        let row: Int
        let red: UInt8
        let green: UInt8
        let blue: UInt8
    }
    let cells: [Cell]
    static let side = 16

    init?(image: CGImage) {
        guard image.width == Self.side, image.height == Self.side,
              let context = CGContext(data: nil, width: Self.side, height: Self.side,
                bitsPerComponent: 8, bytesPerRow: Self.side * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo:
                    CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        context.interpolationQuality = .none
        context.draw(image, in: CGRect(x: 0, y: 0, width: Self.side, height: Self.side))
        guard let data = context.data else { return nil }
        let bytes = data.assumingMemoryBound(to: UInt8.self)
        var result = [Cell]()
        result.reserveCapacity(Self.side * Self.side)
        for row in 0..<Self.side {
            for column in 0..<Self.side {
                let i = row * context.bytesPerRow + column * 4
                let alpha = Int(bytes[i + 3])
                // Discard near-transparent fringes, then fill solid cells.
                guard alpha >= 64 else { continue }
                func channel(_ offset: Int) -> UInt8 {
                    UInt8(min(255, (Int(bytes[i + offset]) * 255 + alpha / 2) / alpha))
                }
                result.append(Cell(column: column, row: row,
                    red: channel(0), green: channel(1), blue: channel(2)))
            }
        }
        guard !result.isEmpty else { return nil }
        cells = result
    }

    func safeRowOffset(_ proposed: Int) -> Int {
        let offset = min(1, max(-1, proposed))
        return cells.allSatisfy { (0..<Self.side).contains($0.row + offset) } ? offset : 0
    }
}

enum EmojiPixelDrawing {
    static let pitch: Double = 4
    static let cellSize: Double = 3.5
    static func rect(column: Int, row: Int) -> CGRect {
        CGRect(x: 2.25 + Double(column) * pitch, y: 2.25 + Double(row) * pitch,
               width: cellSize, height: cellSize)
    }

    static func draw(_ raster: EmojiPixelRaster?, in context: inout GraphicsContext,
                     rowOffset: Int = 0, highlighted: Bool = false) {
        let ink = Color(red: 0.07, green: 0.20, blue: 0.31)
        context.clip(to: Path(CGRect(x: 0, y: 0, width: 68, height: 68)))
        context.fill(Path(CGRect(x: 2, y: 2, width: 64, height: 64)),
                     with: .color(Color(red: 0.94, green: 0.95, blue: 0.925)))
        for row in 0..<16 {
            for column in 0..<16 {
                context.fill(Path(rect(column: column, row: row)), with: .color(ink.opacity(0.045)),
                             style: FillStyle(antialiased: false))
            }
        }
        if let raster {
            let offset = raster.safeRowOffset(rowOffset)
            for cell in raster.cells {
                let colour = Color(.sRGB, red: Double(cell.red) / 255,
                                   green: Double(cell.green) / 255, blue: Double(cell.blue) / 255, opacity: 1)
                context.fill(Path(rect(column: cell.column, row: cell.row + offset)), with: .color(colour),
                             style: FillStyle(antialiased: false))
            }
        } else {
            for column in [5, 7, 9] {
                context.fill(Path(rect(column: column, row: 7)), with: .color(ink.opacity(0.25)),
                             style: FillStyle(antialiased: false))
            }
        }
        let rim = Path(roundedRect: CGRect(x: 0.5, y: 0.5, width: 67, height: 67), cornerRadius: 4)
        context.stroke(rim, with: .color(highlighted
            ? Color(red: 0.67, green: 0.34, blue: 0.07).opacity(0.6) : ink.opacity(0.15)), lineWidth: 1)
    }
}
