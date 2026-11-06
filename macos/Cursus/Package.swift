// swift-tools-version:5.5
// Builds with Swift 5.5+ and the macOS 12 SDK: Xcode 13.1 … 14.2 (or their
// Command Line Tools) run on macOS 12 Monterey, the oldest system Cursus
// supports. Don't raise this without a reason.
import PackageDescription

let package = Package(
    name: "Cursus",
    platforms: [.macOS(.v12)],
    products: [
        .executable(name: "Cursus", targets: ["Cursus"]),
    ],
    targets: [
        // C header + module map over the Rust static library
        // (target/<triple>/release/libcursus_bridge.a, built by ../build.sh,
        // which passes its directory to the linker).
        .systemLibrary(name: "CCursusBridge", path: "Sources/CCursusBridge"),
        .executableTarget(
            name: "Cursus",
            dependencies: ["CCursusBridge"],
            path: "Sources/Cursus"
        ),
    ]
)
