import AppKit
import SwiftUI

/// Our own Settings window. The SwiftUI `Settings` scene can't be opened from a menu bar (LSUIElement)
/// app on macOS 14: `showSettingsWindow:` is ignored there, so the panel's "settings" did nothing.
@MainActor
final class SettingsWindow {
    static let shared = SettingsWindow()
    private var window: NSWindow?

    func show(store: Store) {
        if window == nil {
            let w = NSWindow(contentViewController: NSHostingController(rootView: SettingsView(store: store)))
            w.title = "lobocode · config"
            w.styleMask = [.titled, .closable, .miniaturizable, .fullSizeContentView]
            w.titlebarAppearsTransparent = true
            w.appearance = NSAppearance(named: .darkAqua)
            w.backgroundColor = NSColor(srgbRed: 0x0B / 255, green: 0x0D / 255, blue: 0x10 / 255, alpha: 1)
            w.isReleasedWhenClosed = false
            w.center()
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}
