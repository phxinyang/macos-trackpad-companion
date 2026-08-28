// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TrackpadCompanionSettings",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "TrackpadCompanionSettings", targets: ["TrackpadCompanionSettings"])
    ],
    targets: [
        .executableTarget(name: "TrackpadCompanionSettings")
    ]
)
