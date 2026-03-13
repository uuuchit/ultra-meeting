import SwiftUI

struct MenuBarView: View {
    @ObservedObject var appState: AppState
    @ObservedObject var meetingStore = MeetingStore.shared
    @State private var showSettings = false
    @State private var showRecordingsBrowser = false
    @State private var showZoomAutoStartAlert = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: appState.menuBarIcon)
                Text(appState.menuBarTitle)
                    .font(.headline)
            }
            .padding(.bottom, 4)

            if let msg = appState.errorMessage {
                VStack(alignment: .leading, spacing: 4) {
                    Text(msg)
                        .font(.caption)
                        .foregroundColor(.red)
                    if msg.contains("permission") || msg.contains("Permission") || msg.contains("revoked") {
                        Button("Open System Settings") {
                            if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy") {
                                NSWorkspace.shared.open(url)
                            }
                        }
                        .font(.caption2)
                    }
                }
            }

            Divider()

            if !AppSettings.hasCompletedOnboarding {
                OnboardingSection(appState: appState)
            } else if !appState.recordingAcknowledged {
                ComplianceAckSection(appState: appState)
            } else if !appState.micGranted || !appState.screenGranted {
                PermissionSection(appState: appState)
            } else {
                RecordingSection(appState: appState)
            }

            Divider()

            MeetingListSection(meetingStore: meetingStore, showRecordingsBrowser: $showRecordingsBrowser)

            Button("Open Recordings Folder") {
                openRecordingsFolder()
            }

            if AppSettings.lastRecordingURL() != nil {
                Button("Open Last Recording") {
                    openLastRecording()
                }
            }

            Button("Browse Recordings...") {
                showRecordingsBrowser = true
            }
            .sheet(isPresented: $showRecordingsBrowser) {
                RecordingsBrowserView(isPresented: $showRecordingsBrowser)
                    .onDisappear { meetingStore.refresh() }
            }

            if let meetingApp = MeetingDetection.detectedMeetingApp(),
               appState.recordingState == "idle",
               appState.recordingAcknowledged,
               appState.micGranted,
               appState.screenGranted {
                Button("\(meetingApp) detected - Start?") {
                    showZoomAutoStartAlert = true
                }
                .alert("\(meetingApp) detected", isPresented: $showZoomAutoStartAlert) {
                    Button("Start Recording") { appState.startRecording() }
                    Button("Cancel", role: .cancel) {}
                } message: {
                    Text("\(meetingApp) appears to be running. Start recording?")
                }
            }

            Button("Settings...") {
                showSettings = true
            }
            .sheet(isPresented: $showSettings) {
                SettingsView(isPresented: $showSettings)
            }

            Divider()

            Button("Quit Ultra Meeting") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q", modifiers: .command)
        }
        .frame(width: 280)
        .padding()
        .onAppear {
            appState.checkPermissions()
            appState.syncStateFromRust()
            meetingStore.refresh()
        }
    }

    func openRecordingsFolder() {
        guard let url = AppSettings.recordingsURL, FileManager.default.fileExists(atPath: url.path) else { return }
        NSWorkspace.shared.open(url)
    }

    func openLastRecording() {
        guard let url = AppSettings.lastRecordingURL() else { return }
        NSWorkspace.shared.open(url)
    }
}

struct MeetingListSection: View {
    @ObservedObject var meetingStore: MeetingStore
    @Binding var showRecordingsBrowser: Bool

    var body: some View {
        if !meetingStore.recentMeetings.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                Text("Recent meetings")
                    .font(.caption)
                    .foregroundColor(.secondary)
                ForEach(meetingStore.recentMeetings, id: \.id) { meeting in
                    MeetingRow(meeting: meeting)
                        .onTapGesture {
                            NSWorkspace.shared.open(URL(fileURLWithPath: meeting.recordingPath))
                        }
                }
            }
            .padding(.vertical, 4)
        }
    }
}

struct MeetingRow: View {
    let meeting: MeetingRecord

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(meeting.name)
                    .font(.caption)
                    .lineLimit(1)
                Text(meeting.dateString)
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
            Spacer()
            if meeting.hasTranscript {
                Image(systemName: "doc.text")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
        }
        .padding(.vertical, 2)
    }
}

struct OnboardingSection: View {
    @ObservedObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Welcome to Ultra Meeting")
                .font(.headline)
            Text("Record meetings with your mic and meeting app audio. Transcripts are generated locally.")
                .font(.caption)
                .foregroundColor(.secondary)
            Button("Get Started") {
                AppSettings.hasCompletedOnboarding = true
            }
        }
    }
}

struct ComplianceAckSection: View {
    @ObservedObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Recording Disclosure")
                .font(.headline)
            Text("Ultra Meeting records your mic and meeting app audio locally. No data is sent to the cloud.")
                .font(.caption)
            Text("By recording, you confirm you have appropriate consent from all participants where required by law.")
                .font(.caption2)
                .foregroundColor(.secondary)
            Toggle("I understand", isOn: $appState.recordingAcknowledged)
        }
    }
}

struct PermissionSection: View {
    @ObservedObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Permissions required")
                .font(.caption)
                .foregroundColor(.secondary)
            if !appState.micGranted {
                Button("Grant Microphone") {
                    appState.requestMicPermission()
                }
            }
            if !appState.screenGranted {
                VStack(alignment: .leading, spacing: 4) {
                    Button("Grant Screen Capture") {
                        appState.requestScreenPermission()
                    }
                    Text("Restart the app after granting Screen Recording.")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }
        }
    }
}

struct RecordingSection: View {
    @ObservedObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if appState.recordingState == "recording" {
                Text(formatDuration(appState.recordingDuration))
                    .font(.caption)
                    .foregroundColor(.secondary)
                Button("Stop Recording") {
                    appState.stopRecording()
                }
            } else if appState.recordingState == "processing" {
                VStack(alignment: .leading, spacing: 4) {
                    Text(appState.transcriptionProgress > 0 ? "Transcribing... \(appState.transcriptionProgress)%" : "Processing... (may take a few min)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Button("Skip (transcribe later)") {
                        RustBridge.skipTranscription()
                    }
                    .font(.caption)
                }
            } else if appState.recordingState == "preparing" || appState.recordingState == "stopping" {
                Text(appState.recordingState.capitalized)
                    .font(.caption)
                    .foregroundColor(.secondary)
            } else if appState.recordingState == "idle" {
                Button("Start Recording") {
                    appState.startRecording()
                }
            }
        }
    }

    func formatDuration(_ seconds: TimeInterval) -> String {
        let m = Int(seconds) / 60
        let s = Int(seconds) % 60
        return String(format: "%d:%02d", m, s)
    }
}
