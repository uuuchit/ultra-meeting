// Compliance layer: recording disclosure, legal reminder, audit trail.
// Shown before first capture and in settings.

import SwiftUI

struct ComplianceView: View {
    @Binding var acknowledged: Bool
    @Binding var showLegalReminder: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Recording Disclosure")
                .font(.headline)
            Text("Ultra Meeting records your microphone and meeting app audio locally. No data is sent to the cloud.")
                .font(.body)
            Text("By starting a recording, you confirm you have appropriate consent from all participants where required by law.")
                .font(.caption)
                .foregroundColor(.secondary)
            Toggle("I understand", isOn: $acknowledged)
            if showLegalReminder {
                LegalReminderBanner()
            }
        }
        .padding()
    }
}

struct LegalReminderBanner: View {
    var body: some View {
        Text("Some regions require all-party consent to record. Check your local laws.")
            .font(.caption)
            .foregroundColor(.orange)
            .padding(8)
            .background(Color.orange.opacity(0.2))
            .cornerRadius(6)
    }
}

/// Writes an immutable audit trail entry for each recording start/stop.
struct AuditTrail {
    static func log(event: String, meetingId: String) {
        guard let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("UltraMeeting/audit") else { return }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appendingPathComponent("audit.log")
        let line = "\(ISO8601DateFormatter().string(from: Date())) \(event) \(meetingId)\n"
        if !FileManager.default.fileExists(atPath: file.path) {
            try? line.write(to: file, atomically: true, encoding: .utf8)
        } else if let data = line.data(using: .utf8), let handle = try? FileHandle(forWritingTo: file) {
            defer { try? handle.close() }
            handle.seekToEndOfFile()
            handle.write(data)
        }
    }
}
