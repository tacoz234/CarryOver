// Builds every app icon from icon-source.png. Run from the repo root:
//   swift assets/make-icons.swift
//
// The source has a glow and specks around its rounded square, so only its
// white artwork is kept and redrawn on flat purple. Writes:
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

let purple = CGColor(srgbRed: 100 / 255, green: 2 / 255, blue: 249 / 255, alpha: 1)
let white = CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 1)
/// The source's purple square, measured from the image.
let square = CGRect(x: 41, y: 50, width: 1172, height: 1153)
let size = 1024

func load(_ path: String) -> CGImage {
    let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil)!
    return CGImageSourceCreateImageAtIndex(source, 0, nil)!
}

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
func context(_ width: Int, _ height: Int, opaque: Bool = false) -> CGContext {
    let alpha: CGImageAlphaInfo = opaque ? .noneSkipLast : .premultipliedLast
    let context = CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: alpha.rawValue
    )!
    context.interpolationQuality = .high
    return context
}

/// How white each pixel of the artwork is, as a grayscale mask. Purple has
/// almost no green and white has full green, so green measures the blend.
/// Anything outside the square, like the glow, counts as background.
func artworkMask(_ source: CGImage) -> CGImage {
    let width = source.width, height = source.height
    let drawing = context(width, height)
    drawing.draw(source, in: CGRect(x: 0, y: 0, width: width, height: height))
    let pixels = drawing.data!.bindMemory(to: UInt8.self, capacity: drawing.bytesPerRow * height)
    let inside = CGPath(roundedRect: square.insetBy(dx: 40, dy: 40), cornerWidth: 200, cornerHeight: 200, transform: nil)
    var mask = [UInt8](repeating: 0, count: width * height)
    for y in 0..<height {
        for x in 0..<width where inside.contains(CGPoint(x: x, y: y)) {
            let green = Double(pixels[y * drawing.bytesPerRow + x * 4 + 1])
            mask[y * width + x] = UInt8(max(0, min(255, (green - 2) / 253 * 255)))
        }
    }
    return CGImage(
        width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: width,
        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGBitmapInfo(rawValue: 0),
        provider: CGDataProvider(data: Data(mask) as CFData)!, decode: nil, shouldInterpolate: true, intent: .defaultIntent
    )!
}

/// `rounded` clips to a rounded square, like iOS does to app icons.
/// `template` draws only the artwork, in black on transparent, which macOS
/// recolors for light and dark menu bars.
func render(_ mask: CGImage, size: Int, rounded: Bool, template: Bool = false) -> CGImage {
    let drawing = context(size, size, opaque: !rounded && !template)
    let full = CGRect(x: 0, y: 0, width: size, height: size)
    if rounded {
        let radius = CGFloat(size) * 0.2237
        drawing.addPath(CGPath(roundedRect: full, cornerWidth: radius, cornerHeight: radius, transform: nil))
        drawing.clip()
    }
    if !template {
        drawing.setFillColor(purple)
        drawing.fill(full)
    }
    // Crops to the source's square, centered, and scales it to fill the icon.
    let side = max(square.width, square.height)
    let crop = CGRect(x: square.midX - side / 2, y: square.midY - side / 2, width: side, height: side)
    let scale = CGFloat(size) / side
    // Mask rows run top to bottom; CoreGraphics runs bottom to top.
    let placed = CGRect(
        x: -crop.minX * scale,
        y: -(CGFloat(mask.height) - crop.maxY) * scale,
        width: CGFloat(mask.width) * scale,
        height: CGFloat(mask.height) * scale
    )
    drawing.clip(to: placed, mask: mask)
    drawing.setFillColor(template ? CGColor(gray: 0, alpha: 1) : white)
    drawing.fill(full)
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

let mask = artworkMask(load("assets/icon-source.png"))
let square1024 = render(mask, size: size, rounded: false)
save(square1024, "assets/icon.png")
save(square1024, "ios/CopyBlip/Assets.xcassets/AppIcon.appiconset/icon.png")
save(render(mask, size: size, rounded: true), "assets/icon-rounded.png")
let sizes = [16, 24, 32, 48, 64, 128, 256]
try! ico(sizes.map { ($0, pngData(render(mask, size: $0, rounded: true))) })
    .write(to: URL(fileURLWithPath: "assets/icon.ico"))
save(render(mask, size: 44, rounded: false, template: true), "assets/tray-mac.png")
save(render(mask, size: 32, rounded: true), "assets/tray-windows.png")
print("Wrote icons in assets/ and the iOS app icon.")
