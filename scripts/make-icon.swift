// Draws the Kuterlapa app icon (a paw with a sound wave) and writes an
// .iconset folder with every size macOS needs. `iconutil` turns it into
// an .icns file; the Makefile runs both.
//
//   swift scripts/make-icon.swift <output.iconset>
//
// The drawing uses a 140 x 140 design grid, scaled to each size.

import AppKit

let background = NSColor(srgbRed: 0.957, green: 0.914, blue: 0.847, alpha: 1)  // #F4E9D8
let paw = NSColor(srgbRed: 0.710, green: 0.396, blue: 0.114, alpha: 1)  // #B5651D
let wave = NSColor(srgbRed: 0.114, green: 0.620, blue: 0.459, alpha: 1)  // #1D9E75

/// Draws the icon into the current context, in a square of `side` points.
func draw(side: CGFloat) {
    // macOS icons leave a margin around the rounded square: 824 of 1024.
    let inset = side * 100 / 1024
    let box = side - 2 * inset
    let unit = box / 140
    // Design grid: x right, y down, 0...140. AppKit: y up.
    func point(_ x: CGFloat, _ y: CGFloat) -> NSPoint {
        NSPoint(x: inset + x * unit, y: inset + (140 - y) * unit)
    }
    func ellipse(_ cx: CGFloat, _ cy: CGFloat, _ rx: CGFloat, _ ry: CGFloat) {
        let c = point(cx, cy)
        NSBezierPath(ovalIn: NSRect(x: c.x - rx * unit, y: c.y - ry * unit,
                                    width: 2 * rx * unit, height: 2 * ry * unit)).fill()
    }

    background.setFill()
    NSBezierPath(roundedRect: NSRect(x: inset, y: inset, width: box, height: box),
                 xRadius: box * 0.225, yRadius: box * 0.225).fill()

    paw.setFill()
    ellipse(62, 88, 26, 22)
    ellipse(34, 58, 9, 12)
    ellipse(52, 44, 9, 12)
    ellipse(74, 44, 9, 12)
    ellipse(92, 58, 9, 12)

    wave.setStroke()
    for (x, top, bottom, bulge) in [(102.0, 76.0, 100.0, 8.0), (112, 68, 108, 12), (122, 60, 116, 16)] {
        let arc = NSBezierPath()
        arc.move(to: point(x, top))
        arc.curve(to: point(x, bottom),
                  controlPoint1: point(x + bulge, top + (bottom - top) * 0.3),
                  controlPoint2: point(x + bulge, top + (bottom - top) * 0.7))
        arc.lineWidth = 5 * unit
        arc.lineCapStyle = .round
        arc.stroke()
    }
}

func png(pixels: Int) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw(side: CGFloat(pixels))
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: make-icon.swift <output.iconset>\n".data(using: .utf8)!)
    exit(2)
}
let out = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
for size in [16, 32, 128, 256, 512] {
    try png(pixels: size).write(to: out.appendingPathComponent("icon_\(size)x\(size).png"))
    try png(pixels: size * 2).write(to: out.appendingPathComponent("icon_\(size)x\(size)@2x.png"))
}
