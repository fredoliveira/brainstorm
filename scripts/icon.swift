// Makes the app icon from square artwork: the art cut to the rounded tile macOS app icons use,
// with a soft shadow, as assets/icon.png and assets/icon.icns.
// Usage: swift scripts/icon.swift assets/icon-art.png assets
//
// The art is an homage to Tony DiTerlizzi's painting for the card Brainstorm (the woman with her
// eyes closed, the lights around her head), made with an image model rather than copied from it.
import AppKit

let art = NSImage(contentsOfFile: CommandLine.arguments[1])!
let out = URL(fileURLWithPath: CommandLine.arguments[2])

/// The icon at `size` pixels square, drawn from scratch at that size rather than scaled down.
func icon(_ size: Int) -> NSBitmapImageRep {
    let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    let context = NSGraphicsContext.current!.cgContext
    context.interpolationQuality = .high
    // Everything below is in the 1024-point design grid, except shadows: Core Graphics draws
    // those in pixels whatever the scale, so they are scaled by hand.
    let scale = Double(size) / 1024
    context.scaleBy(x: scale, y: scale)

    // The tile, inset and rounded the way macOS app icons are.
    let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
    let shape = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)
    context.saveGState()
    context.setShadow(
        offset: CGSize(width: 0, height: -10 * scale), blur: 28 * scale, color: CGColor(gray: 0, alpha: 0.4))
    context.addPath(shape)
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fillPath()
    context.restoreGState()

    context.addPath(shape)
    context.clip()
    art.draw(in: tile, from: .zero, operation: .copy, fraction: 1)

    NSGraphicsContext.restoreGraphicsState()
    return bitmap
}

func png(_ bitmap: NSBitmapImageRep, _ url: URL) {
    try! bitmap.representation(using: .png, properties: [:])!.write(to: url)
}

png(icon(1024), out.appendingPathComponent("icon.png"))

// An iconset of every size macOS asks for, which iconutil packs into the .icns.
let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("Brainstorm.iconset")
try? FileManager.default.removeItem(at: iconset)
try! FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    png(icon(points), iconset.appendingPathComponent("icon_\(points)x\(points).png"))
    png(icon(points * 2), iconset.appendingPathComponent("icon_\(points)x\(points)@2x.png"))
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", out.appendingPathComponent("icon.icns").path]
try! iconutil.run()
iconutil.waitUntilExit()
precondition(iconutil.terminationStatus == 0, "iconutil failed")
try? FileManager.default.removeItem(at: iconset)
