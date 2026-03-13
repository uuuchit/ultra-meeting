// Ultra Meeting - Menu Bar App
// Slice A: Shell, permissions, start/stop

import SwiftUI

@main
struct UltraMeetingApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @StateObject private var appState = AppState()

    var body: some Scene {
        MenuBarExtra(
            appState.menuBarTitle,
            systemImage: appState.menuBarIcon
        ) {
            MenuBarView(appState: appState)
        }
        .menuBarExtraStyle(.window)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        if let err = RustBridge.initCore() {
            NSLog("Ultra Meeting: Rust core init failed: %@", err)
        }
    }
}
