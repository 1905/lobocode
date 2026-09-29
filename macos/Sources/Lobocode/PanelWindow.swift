import AppKit
import SwiftUI

/// The panel as a normal window. A full menu bar hides status items behind the notch without any error,
/// so opening the app (launch, or Finder/Spotlight/Launchpad while running) always shows this window too.
@MainActor
final class PanelWindow {
    static let shared = PanelWindow()
    private var window: NSWindow?

    func show(store: Store) {
        if window == nil {
            let view = PanelView(store: store, openSettings: { SettingsWindow.shared.show(store: store) })
                .onAppear { store.panelOpen = true; Task { await store.refresh(models: true) } }
                .onDisappear { store.panelOpen = false }
            let w = NSWindow(contentViewController: NSHostingController(rootView: view))
            w.title = "lobocode"
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

/// Opens the panel window at launch and on every reopen (a menu bar app gets no Dock click, but Finder,
/// Spotlight and Launchpad send a reopen). `--background` skips the launch window (login items).
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        guard !CommandLine.arguments.contains("--background") else { return }
        MainActor.assumeIsolated { PanelWindow.shared.show(store: Store.shared) }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        MainActor.assumeIsolated { PanelWindow.shared.show(store: Store.shared) }
        return true
    }
}
