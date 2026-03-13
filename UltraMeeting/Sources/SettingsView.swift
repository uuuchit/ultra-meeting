import SwiftUI
import AppKit

struct SettingsView: View {
    @AppStorage("defaultMeetingName") private var meetingName = "Meeting"
    @AppStorage("storagePath") private var storagePath = ""
    @AppStorage("qmdSearchEnabled") private var qmdEnabled = false
    @Binding var isPresented: Bool
    @State private var qmdAddResult: String?
    @State private var qmdUpdateResult: String?
    @State private var qmdEmbedResult: String?
    
    private var defaultStoragePath: String {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first?
            .appendingPathComponent("UltraMeeting").path ?? ""
    }
    
    var body: some View {
        Form {
            Section("Recording") {
                TextField("Default meeting name", text: $meetingName)
                HStack {
                    TextField("Storage root (recordings/ and transcripts/ created inside)", text: $storagePath)
                        .textFieldStyle(.roundedBorder)
                    Button("Browse...") {
                        selectStoragePath()
                    }
                }
                if storagePath.isEmpty {
                    Text("Default: \(defaultStoragePath)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }
            Section("Search") {
                Toggle("Enable QMD (semantic + keyword search)", isOn: $qmdEnabled)
                if qmdEnabled && !QMDService.shared.isQMDInstalled() {
                    Text("QMD not installed. Run: npm install -g @tobilu/qmd")
                        .font(.caption)
                        .foregroundColor(.orange)
                    Link("QMD on GitHub", destination: URL(string: "https://github.com/tobi/qmd")!)
                        .font(.caption)
                }
                if QMDService.shared.isQMDInstalled() {
                    HStack {
                        Button("Add transcripts to QMD") {
                            addTranscriptsToQMD()
                        }
                        Button("Re-index") {
                            reindexQMD()
                        }
                        Button("Generate embeddings") {
                            embedQMD()
                        }
                    }
                    .buttonStyle(.borderless)
                    if let msg = qmdAddResult ?? qmdUpdateResult ?? qmdEmbedResult {
                        Text(msg)
                            .font(.caption)
                            .foregroundColor(msg.contains("error") || msg.contains("failed") ? .red : .secondary)
                    }
                }
            }
            Section("Privacy (coming soon)") {
                Toggle("Auto-generate summaries", isOn: .constant(false))
                    .disabled(true)
                Text("Summaries will be generated from transcripts locally.")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text("Retention & encryption: planned for a future release.")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            Section {
                Button("Done") {
                    isPresented = false
                }
            }
        }
        .formStyle(.grouped)
        .frame(minWidth: 400, minHeight: 200)
    }
    
    private func addTranscriptsToQMD() {
        qmdAddResult = nil
        qmdUpdateResult = nil
        qmdEmbedResult = nil
        guard let path = AppSettings.transcriptsURL?.path else {
            qmdAddResult = "No transcripts path"
            return
        }
        switch QMDService.shared.addTranscriptsCollection(path: path) {
        case .success:
            _ = QMDService.shared.addContext()
            qmdAddResult = "Added"
        case .failure(let e): qmdAddResult = "Error: \(e.message)"
        }
    }

    private func reindexQMD() {
        qmdAddResult = nil
        qmdUpdateResult = nil
        qmdEmbedResult = nil
        switch QMDService.shared.updateIndex() {
        case .success: qmdUpdateResult = "Re-indexed"
        case .failure(let e): qmdUpdateResult = "Error: \(e.message)"
        }
    }

    private func embedQMD() {
        qmdAddResult = nil
        qmdUpdateResult = nil
        qmdEmbedResult = nil
        qmdEmbedResult = "Running (may take minutes)..."
        DispatchQueue.global(qos: .userInitiated).async {
            let result = QMDService.shared.embed()
            DispatchQueue.main.async {
                switch result {
                case .success: self.qmdEmbedResult = "Done"
                case .failure(let e): self.qmdEmbedResult = "Error: \(e.message)"
                }
            }
        }
    }

    private func selectStoragePath() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            storagePath = url.path
        }
    }
}

