// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CopyBlipKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "CopyBlipKit", targets: ["CopyBlipKit"])],
    targets: [
        .target(name: "CopyBlipKit"),
        .testTarget(name: "CopyBlipKitTests", dependencies: ["CopyBlipKit"]),
    ]
)
