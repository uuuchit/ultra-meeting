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

        WindowGroup("Ultra Meeting") {
            ControlWindowContent(appState: appState)
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 360, height: 320)
        .commands {
            CommandMenu("Recording") {
                Button("Stop Recording") {
                    appState.stopRecording()
                }
                .disabled(appState.recordingState != "recording")
                .keyboardShortcut(".", modifiers: [.command])
            }
        }

        // Standalone windows prevent closure when menu bar popover loses focus
        Window("Recordings", id: "recordings") {
            RecordingsBrowserWindowContent()
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 600, height: 500)

        Window("Settings", id: "settings") {
            SettingsWindowContent()
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 450, height: 350)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        if let err = RustBridge.initCore() {
            NSLog("Ultra Meeting: Rust core init failed: %@", err)
        }
    }
}
