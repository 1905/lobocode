// swift-tools-version:5.10
// lobocode.app: menu bar front end for the lobo CLI. Build the .app with `make mac` from the repo root.
import PackageDescription

let package = Package(
    name: "Lobocode",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(name: "Lobocode", path: "Sources/Lobocode"),
        .testTarget(name: "LobocodeTests", dependencies: ["Lobocode"], path: "Tests/LobocodeTests", resources: [.copy("Fixtures")]),
    ]
)
