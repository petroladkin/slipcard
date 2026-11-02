// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "VocabPopup",
    platforms: [.macOS(.v14)],
    targets: [
        // Logic with no AppKit dependency, so tests run without a display.
        .target(name: "VocabPopupCore"),
        .executableTarget(name: "VocabPopup", dependencies: ["VocabPopupCore"]),
        .testTarget(name: "VocabPopupCoreTests", dependencies: ["VocabPopupCore"]),
    ]
)
