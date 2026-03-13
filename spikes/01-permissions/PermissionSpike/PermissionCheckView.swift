// Phase 0 Spike: Permission checks and request flow
// Validates: mic grant immediate? screen grant requires restart?

import AVFoundation
import CoreGraphics
import SwiftUI

struct PermissionCheckView: View {
    @State var micStatus: String = "Checking..."
    @State var screenStatus: String = "Checking..."
    @State var screenRequested = false

    var body: some View {
        VStack(spacing: 20) {
            Text("Permission Lifecycle Spike")
                .font(.headline)
            Group {
                HStack {
                    Text("Microphone:")
                    Text(micStatus).foregroundColor(statusColor(micStatus))
                }
                HStack {
                    Text("Screen Capture:")
                    Text(screenStatus).foregroundColor(statusColor(screenStatus))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            Button("Check Permissions") {
                checkPermissions()
            }

            if !screenRequested {
                Button("Request Screen Capture") {
                    requestScreenCapture()
                }
            }

            Text("Note: Screen capture grant may require app restart. Document behavior.")
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding()
        }
        .frame(width: 400, height: 300)
        .onAppear { checkPermissions() }
    }

    func statusColor(_ s: String) -> Color {
        if s.contains("Granted") { return .green }
        if s.contains("Denied") { return .red }
        return .primary
    }

    func checkPermissions() {
        // Microphone
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: micStatus = "Granted"
        case .denied: micStatus = "Denied (open System Preferences > Privacy)"
        case .notDetermined:
            micStatus = "Not determined"
            AVCaptureDevice.requestAccess(for: .audio) { granted in
                DispatchQueue.main.async {
                    micStatus = granted ? "Granted (immediate)" : "Denied"
                }
            }
        case .restricted: micStatus = "Restricted"
        @unknown default: micStatus = "Unknown"
        }

        // Screen capture (macOS 11+)
        #if os(macOS)
        if #available(macOS 11.0, *) {
            let hasAccess = CGPreflightScreenCaptureAccess()
            screenStatus = hasAccess ? "Granted" : "Denied or not yet granted"
        } else {
            screenStatus = "Unsupported"
        }
        #endif
    }

    func requestScreenCapture() {
        #if os(macOS)
        if #available(macOS 11.0, *) {
            screenRequested = true
            let granted = CGRequestScreenCaptureAccess()
            DispatchQueue.main.async {
                screenStatus = granted ? "Granted (restart may be required)" : "Denied"
                checkPermissions()
            }
        }
        #endif
    }
}
