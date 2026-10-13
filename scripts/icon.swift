import AppKit

let destination = CommandLine.arguments[1]
let source = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent()
    .appendingPathComponent("assets/tervia.svg")
let image = NSImage(contentsOf: source)!

func render(_ dimension: Int) -> Data {
    // Render the shared SVG at a fixed pixel size, independent of the display's Retina scale.
    let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: dimension, pixelsHigh: dimension,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    )!
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: bitmap)!
    NSGraphicsContext.current = context
    context.imageInterpolation = .high
    let bounds = NSRect(x: 0, y: 0, width: dimension, height: dimension)
    NSColor.clear.setFill()
    bounds.fill(using: .copy)
    image.draw(in: bounds)
    NSGraphicsContext.restoreGraphicsState()
    return bitmap.representation(using: .png, properties: [:])!
}

try render(1024).write(to: URL(fileURLWithPath: destination))

if CommandLine.arguments.count > 2 {
    let sizes = [16, 24, 32, 48, 64, 128, 256]
    let frames = sizes.map(render)
    var icon = Data()
    func append<T: FixedWidthInteger>(_ value: T) {
        var littleEndian = value.littleEndian
        withUnsafeBytes(of: &littleEndian) { icon.append(contentsOf: $0) }
    }
    append(UInt16(0))
    append(UInt16(1))
    append(UInt16(sizes.count))
    var offset = UInt32(6 + 16 * sizes.count)
    for (size, frame) in zip(sizes, frames) {
        // ICO represents a 256-pixel dimension as zero in its one-byte directory fields.
        let dimension = UInt8(size == 256 ? 0 : size)
        icon.append(contentsOf: [dimension, dimension, 0, 0])
        append(UInt16(1))
        append(UInt16(32))
        append(UInt32(frame.count))
        append(offset)
        offset += UInt32(frame.count)
    }
    for frame in frames { icon.append(frame) }
    try icon.write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
}
