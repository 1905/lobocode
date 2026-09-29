import AppKit
import SwiftUI

struct LobocodeApp: App {
    @StateObject private var store = Store.shared

    init() {
        // `Lobocode --settings` opens the config window at launch (also handy from a terminal).
        if CommandLine.arguments.contains("--settings") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { SettingsWindow.shared.show(store: Store.shared) }
        }
    }

    var body: some Scene {
        MenuBarExtra {
            PanelView(store: store, openSettings: openSettings)
                .onAppear { store.panelOpen = true; Task { await store.refresh(models: true) } }
                .onDisappear { store.panelOpen = false }
        } label: {
            MenuLabel(store: store)
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView(store: store)
        }
    }

    private func openSettings() {
        SettingsWindow.shared.show(store: store)
    }
}
