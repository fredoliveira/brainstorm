// Draws the placeholder app icon: three cards fanned on a night-blue tile, for the three that
// Brainstorm draws. Usage: swift scripts/icon.swift assets/icon.png
import AppKit

let size = 1024.0
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let context = NSGraphicsContext.current!.cgContext

// The tile, inset and rounded the way macOS app icons are.
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
context.addPath(CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil))
context.clip()
let colors = [CGColor(red: 0.10, green: 0.16, blue: 0.36, alpha: 1), CGColor(red: 0.03, green: 0.05, blue: 0.14, alpha: 1)]
let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors as CFArray, locations: [0, 1])!
context.drawLinearGradient(gradient, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])

// Three cards, the outer two tilted away from the middle one.
let card = CGRect(x: -105, y: -150, width: 210, height: 300)
let fan: [(x: Double, y: Double, turn: Double, shade: Double)] = [(372, 480, 0.24, 0.80), (652, 480, -0.24, 0.80), (512, 520, 0, 1)]
for (x, y, turn, shade) in fan {
    context.saveGState()
    context.translateBy(x: x, y: y)
    context.rotate(by: turn)
    context.setShadow(offset: CGSize(width: 0, height: -10), blur: 30, color: CGColor(gray: 0, alpha: 0.5))
    context.setFillColor(CGColor(red: 0.93 * shade, green: 0.95 * shade, blue: 1.0 * shade, alpha: 1))
    context.addPath(CGPath(roundedRect: card, cornerWidth: 26, cornerHeight: 26, transform: nil))
    context.fillPath()
    context.restoreGState()
}
image.unlockFocus()

let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
try! bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
