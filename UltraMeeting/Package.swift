// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "UltraMeeting",
    platforms: [.macOS(.v15)],
    products: [
        .executable(name: "UltraMeeting", targets: ["UltraMeeting"]),
    ],
    targets: [
        .executableTarget(
            name: "UltraMeeting",
            path: "Sources"
        ),
    ]
)
