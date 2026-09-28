import SwiftUI

// `Lobocode --render <dir>` writes PNGs of every panel state (design check, no CLI needed).
if let i = CommandLine.arguments.firstIndex(of: "--render"), i + 1 < CommandLine.arguments.count {
    MainActor.assumeIsolated { Renderer.run(to: URL(fileURLWithPath: CommandLine.arguments[i + 1])) }
    exit(0)
}
LobocodeApp.main()
