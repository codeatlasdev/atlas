// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "Atlas",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Atlas",
            path: "Sources/Atlas"
        ),
        .testTarget(
            name: "AtlasTests",
            dependencies: ["Atlas"],
            path: "Tests/AtlasTests"
        ),
    ]
)
