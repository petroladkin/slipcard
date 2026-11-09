// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Slipcard",
    platforms: [.macOS(.v14)],
    targets: [
        // Logic with no AppKit dependency, so tests run without a display.
        .target(name: "SlipcardCore"),
        .executableTarget(name: "Slipcard", dependencies: ["SlipcardCore"]),
        .testTarget(name: "SlipcardCoreTests", dependencies: ["SlipcardCore"]),
    ]
)
