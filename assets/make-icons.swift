// Draws every app icon: a clipboard with an arrow carrying over its top, in
// white on a coral gradient. Run from the repo root:
//   swift assets/make-icons.swift
//
// Writes:
//   assets/icon.png          1024px square, no transparency, for iOS (which rounds it)
//   assets/icon-rounded.png  1024px with transparent rounded corners, for docs
//   assets/icon.ico          16 to 256px, for the Windows program
//   assets/tray-mac.png      44px template image for the macOS menu bar
//   assets/tray-windows.png  32px for the Windows notification area
//   ios/CopyBlip/Assets.xcassets/AppIcon.appiconset/icon.png
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// #FF9158 at the top to #F0533C at the bottom, as in the apps.
let coralTop = CGColor(srgbRed: 1.0, green: 0.569, blue: 0.345, alpha: 1)
let coralBottom = CGColor(srgbRed: 0.941, green: 0.325, blue: 0.235, alpha: 1)
let white = CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 1)

func save(_ image: CGImage, _ path: String) {
    let url = URL(fileURLWithPath: path)
    try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, image, nil)
    precondition(CGImageDestinationFinalize(destination), "couldn't write \(path)")
}

func pngData(_ image: CGImage) -> Data {
    let data = NSMutableData()
    let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, image, nil)
    CGImageDestinationFinalize(destination)
    return data as Data
}

/// iOS rejects app icons with an alpha channel, so opaque ones have none.
func context(_ size: Int, opaque: Bool = false) -> CGContext {
    let alpha: CGImageAlphaInfo = opaque ? .noneSkipLast : .premultipliedLast
    let context = CGContext(
        data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: alpha.rawValue
    )!
    context.interpolationQuality = .high
    return context
}

/// The artwork, as paths in a 1×1 square with y up. `bold` thickens the
/// strokes, so the small tray icons stay legible.
func artwork(in drawing: CGContext, size: CGFloat, bold: Bool) {
    drawing.saveGState()
    drawing.scaleBy(x: size, y: size)
    let stroke: CGFloat = bold ? 0.085 : 0.062
    drawing.setLineWidth(stroke)
    drawing.setLineCap(.round)
    drawing.setLineJoin(.round)

    // The board, open at the top where the clip sits.
    let board = CGRect(x: 0.25, y: 0.14, width: 0.50, height: 0.64)
    drawing.addPath(CGPath(roundedRect: board, cornerWidth: 0.08, cornerHeight: 0.08, transform: nil))
    drawing.strokePath()
    // The clip, with a gap around it so it reads at small sizes.
    let clip = CGRect(x: 0.385, y: 0.725, width: 0.23, height: 0.12)
    drawing.setBlendMode(.clear)
    drawing.addPath(CGPath(roundedRect: clip.insetBy(dx: -0.03, dy: -0.03), cornerWidth: 0.06, cornerHeight: 0.06, transform: nil))
    drawing.fillPath()
    drawing.setBlendMode(.normal)
    drawing.addPath(CGPath(roundedRect: clip, cornerWidth: 0.04, cornerHeight: 0.04, transform: nil))
    drawing.fillPath()

    // An arrow arcing up and over, from left to right: carrying it over.
    // It lands heading straight down, so the head points down.
    let start = CGPoint(x: 0.36, y: 0.27)
    let end = CGPoint(x: 0.59, y: 0.33)
    drawing.move(to: start)
    drawing.addCurve(to: end, control1: CGPoint(x: 0.36, y: 0.64), control2: CGPoint(x: 0.59, y: 0.64))
    drawing.strokePath()
    let head: CGFloat = bold ? 0.085 : 0.08
    drawing.move(to: CGPoint(x: end.x - head, y: end.y + head * 0.2))
    drawing.addLine(to: CGPoint(x: end.x, y: end.y - head * 0.8))
    drawing.addLine(to: CGPoint(x: end.x + head, y: end.y + head * 0.2))
    drawing.strokePath()
    drawing.restoreGState()
}

/// `rounded` clips to a rounded square, like iOS does to app icons.
/// `template` draws only the artwork, in black on transparent, which macOS
/// recolors for light and dark menu bars.
func render(size: Int, rounded: Bool, template: Bool = false) -> CGImage {
    let side = CGFloat(size)
    let drawing = context(size, opaque: !rounded && !template)
    let full = CGRect(x: 0, y: 0, width: side, height: side)
    if rounded {
        let radius = side * 0.2237
        drawing.addPath(CGPath(roundedRect: full, cornerWidth: radius, cornerHeight: radius, transform: nil))
        drawing.clip()
    }
    // Small icons get bolder strokes and less padding.
    let small = size <= 48
    if !template {
        let gradient = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: [coralBottom, coralTop] as CFArray, locations: [0, 1])!
        drawing.drawLinearGradient(gradient, start: .zero, end: CGPoint(x: 0, y: side), options: [])
    }
    // The artwork draws in its own layer, so its cut-outs don't cut the background.
    let layer = context(size)
    layer.setStrokeColor(template ? CGColor(gray: 0, alpha: 1) : white)
    layer.setFillColor(template ? CGColor(gray: 0, alpha: 1) : white)
    if small || template {
        // Fills more of a small icon, since there's no room to spare.
        layer.translateBy(x: -side * 0.06, y: -side * 0.04)
        layer.scaleBy(x: 1.12, y: 1.12)
    }
    artwork(in: layer, size: side, bold: small || template)
    drawing.draw(layer.makeImage()!, in: full)
    return drawing.makeImage()!
}

/// An .ico holding PNG images, which Windows Vista and later read.
func ico(_ images: [(Int, Data)]) -> Data {
    var data = Data()
    func u16(_ value: Int) { withUnsafeBytes(of: UInt16(value).littleEndian) { data.append(contentsOf: $0) } }
    func u32(_ value: Int) { withUnsafeBytes(of: UInt32(value).littleEndian) { data.append(contentsOf: $0) } }
    u16(0); u16(1); u16(images.count)
    var offset = 6 + 16 * images.count
    for (size, png) in images {
        data.append(UInt8(size % 256)); data.append(UInt8(size % 256)) // 256 is written as 0
        data.append(0); data.append(0)
        u16(1); u16(32); u32(png.count); u32(offset)
        offset += png.count
    }
    images.forEach { data.append($0.1) }
    return data
}

let square1024 = render(size: 1024, rounded: false)
save(square1024, "assets/icon.png")
save(square1024, "ios/CopyBlip/Assets.xcassets/AppIcon.appiconset/icon.png")
save(render(size: 1024, rounded: true), "assets/icon-rounded.png")
let sizes = [16, 24, 32, 48, 64, 128, 256]
try! ico(sizes.map { ($0, pngData(render(size: $0, rounded: true))) })
    .write(to: URL(fileURLWithPath: "assets/icon.ico"))
save(render(size: 44, rounded: false, template: true), "assets/tray-mac.png")
save(render(size: 32, rounded: true), "assets/tray-windows.png")
print("Wrote icons in assets/ and the iOS app icon.")
