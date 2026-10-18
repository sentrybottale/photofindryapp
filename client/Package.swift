// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "PhotoFindryClient",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "PhotoFindryBrowserHost", targets: ["PhotoFindryBrowserHost"])],
    targets: [
        .executableTarget(name: "PhotoFindryBrowserHost", dependencies: ["Sparkle"]),
        .testTarget(name: "PhotoFindryBrowserHostTests", dependencies: ["PhotoFindryBrowserHost"], path: "tests/PhotoFindryBrowserHostTests"),
        .binaryTarget(name: "Sparkle", url: "https://github.com/sparkle-project/Sparkle/releases/download/2.10.0/Sparkle-for-Swift-Package-Manager.zip",
                      checksum: "17e28312b8e18ab7cdbbe09a6fb28cc55a5479ec6c371dbc07cdecd2a14fd959")
    ]
)
