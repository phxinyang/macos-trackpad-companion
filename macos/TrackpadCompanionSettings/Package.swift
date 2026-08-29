// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TrackpadCompanionSettings",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(
            url: "https://github.com/jaywcjlove/PermissionFlow.git",
            exact: "2.11.2"
        )
    ],
    products: [
        .executable(name: "TrackpadCompanionSettings", targets: ["TrackpadCompanionSettings"])
    ],
    targets: [
        .executableTarget(
            name: "TrackpadCompanionSettings",
            dependencies: [
                .product(name: "PermissionFlow", package: "PermissionFlow")
            ],
            linkerSettings: [
                .linkedFramework("Network"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("ServiceManagement")
            ]
        )
    ]
)
