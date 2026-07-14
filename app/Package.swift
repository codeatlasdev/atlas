// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "Atlas",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", from: "1.14.0"),
    ],
    targets: [
        .executableTarget(
            name: "Atlas",
            dependencies: [
                .product(name: "SwiftTerm", package: "SwiftTerm"),
            ],
            path: "Sources/Atlas"
        ),
        .testTarget(
            name: "AtlasTests",
            dependencies: ["Atlas"],
            path: "Tests/AtlasTests"
        ),
    ]
)
