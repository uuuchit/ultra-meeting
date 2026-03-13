import AppKit

/// Detects when meeting apps (Zoom, etc.) are running for potential auto-start UX.
enum MeetingDetection {
    static func isZoomRunning() -> Bool {
        NSWorkspace.shared.runningApplications.contains { app in
            app.bundleIdentifier == "us.zoom.xos" || app.localizedName?.lowercased().contains("zoom") == true
        }
    }

    static func detectedMeetingApp() -> String? {
        if isZoomRunning() { return "Zoom" }
        // Extensible: Google Meet in Chrome, etc.
        return nil
    }
}
