import AppKit

// Original code-drawn PhotoFindry brand asset. No external images or fonts.
let destination = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)

func color(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat) -> NSColor {
    NSColor(srgbRed: red, green: green, blue: blue, alpha: 1)
}

// A photograph and search glass drawn as solid LCD cells. The same master
// supplies the Dock icon and BrandMark. There are no fonts or bitmap glyphs.
func drawIcon(pixels: Int) {
    let ink = color(0.07, 0.20, 0.31)
    let off = color(0.89, 0.915, 0.895)
    let amber = color(0.77, 0.43, 0.12)
    let tile = NSBezierPath(roundedRect: NSRect(x: 68, y: 68, width: 888, height: 888), xRadius: 190, yRadius: 190)
    NSColor.white.setFill(); tile.fill()
    let gridSize = 24
    var cells = Array(repeating: Array(repeating: 0, count: gridSize), count: gridSize)
    func paint(_ x: Int, _ y: Int, _ value: Int = 1) {
        if (0..<gridSize).contains(x), (0..<gridSize).contains(y) { cells[y][x] = value }
    }
    // Photo frame, sun and mountains.
    for x in 2...18 { paint(x, 4); paint(x, 17) }
    for y in 4...17 { paint(2, y); paint(18, y) }
    for y in 7...8 { for x in 5...6 { paint(x, y, 2) } }
    for (x,y) in [(4,14),(5,13),(6,12),(7,11),(8,12),(9,13),(10,12),(11,11),(12,10),(13,9),(14,10),(15,11),(16,12)] {
        paint(x, y); paint(x, y+1)
    }
    // Pixel-ring lens overlaps the photograph, with a clean interior.
    for y in 11...20 { for x in 11...20 {
        let distance = (Double(x)-15.5)*(Double(x)-15.5) + (Double(y)-15.5)*(Double(y)-15.5)
        if distance <= 21 { paint(x,y,distance >= 10 ? 1 : 0) }
    } }
    for n in 0...3 { paint(19+n,19+n); paint(20+n,19+n); paint(19+n,20+n) }

    let scale = CGFloat(pixels) / 1024
    let pitch: CGFloat = 30
    let origin: CGFloat = 152
    let gap: CGFloat = pixels >= 64 ? max(1 / scale, 2.5) : 0
    let context = NSGraphicsContext.current!.cgContext
    context.saveGState(); context.setShouldAntialias(false)
    for y in 0..<gridSize { for x in 0..<gridSize {
        // Suppress the inactive grid only at tiny system icon sizes.
        if cells[y][x] == 0 && pixels < 64 { continue }
        let left = ((origin + CGFloat(x)*pitch) * scale).rounded() / scale
        let bottom = ((origin + CGFloat(gridSize-1-y)*pitch) * scale).rounded() / scale
        let right = ((origin + CGFloat(x+1)*pitch-gap) * scale).rounded() / scale
        let top = ((origin + CGFloat(gridSize-y)*pitch-gap) * scale).rounded() / scale
        (cells[y][x] == 1 ? ink : cells[y][x] == 2 ? amber : off).setFill()
        NSRect(x: left, y: bottom, width: max(1/scale,right-left), height: max(1/scale,top-bottom)).fill()
    } }
    context.restoreGState()
}

for logicalSize in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = logicalSize * scale
        let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
                                      bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
                                      isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
        NSGraphicsContext.current!.cgContext.scaleBy(x: CGFloat(pixels) / 1024, y: CGFloat(pixels) / 1024)
        drawIcon(pixels: pixels)
        NSGraphicsContext.restoreGraphicsState()
        let suffix = scale == 2 ? "@2x" : ""
        try bitmap.representation(using: .png, properties: [:])!.write(to: destination.appendingPathComponent("icon_\(logicalSize)x\(logicalSize)\(suffix).png"))
    }
}
