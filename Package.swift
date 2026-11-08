// swift-tools-version:5.10
// Swift 5 language mode: the app is small and single-threaded around the
// main run loop, so Swift 6 strict concurrency checks would add noise.

import PackageDescription

let package = Package(
    name: "Kuterlapa",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Kuterlapa",
            path: "Sources/Kuterlapa"
        ),
    ]
)
