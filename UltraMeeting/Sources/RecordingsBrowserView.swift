import SwiftUI

enum SearchMode: String, CaseIterable {
    case keyword
    case qmd
}

struct RecordingsBrowserView: View {
    @Binding var isPresented: Bool
    @ObservedObject var meetingStore = MeetingStore.shared
    @State private var selectedMeeting: MeetingRecord?
    @State private var searchQuery = ""
    @State private var searchMode: SearchMode = .keyword
    @State private var qmdResults: [MeetingRecord] = []
    @State private var isSearchingQMD = false
    @State private var qmdSearchError: String?
    @State private var refreshTrigger = 0

    private var displayedMeetings: [MeetingRecord] {
        if searchMode == .qmd && AppSettings.qmdSearchEnabled && QMDService.shared.isQMDInstalled() {
            return qmdResults
        }
        return meetingStore.searchTranscripts(query: searchQuery)
    }

    private func runSearch() {
        let q = searchQuery.trimmingCharacters(in: .whitespaces)
        if q.isEmpty {
            qmdResults = meetingStore.meetings
            return
        }
        if searchMode == .qmd && QMDService.shared.isQMDInstalled() {
            isSearchingQMD = true
            qmdSearchError = nil
            DispatchQueue.global(qos: .userInitiated).async {
                let result = QMDService.shared.query(query: q)
                DispatchQueue.main.async {
                    isSearchingQMD = false
                    switch result {
                    case .success(let results):
                        let meetingIds = Set(results.compactMap { qmdResult in
                            let url = URL(fileURLWithPath: qmdResult.path)
                            let sessionId = url.deletingLastPathComponent().lastPathComponent
                            return meetingStore.meetings.first { $0.id == sessionId }?.id
                        })
                        qmdResults = meetingStore.meetings.filter { meetingIds.contains($0.id) }
                        if qmdResults.isEmpty && !results.isEmpty {
                            qmdResults = meetingStore.meetings
                        }
                    case .failure(let e):
                        qmdSearchError = e.message
                        qmdResults = meetingStore.meetings
                    }
                }
            }
        } else {
            qmdResults = meetingStore.meetings
        }
    }

    var body: some View {
        NavigationSplitView {
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    TextField("Search transcripts...", text: $searchQuery)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { runSearch() }
                    if AppSettings.qmdSearchEnabled && QMDService.shared.isQMDInstalled() {
                        Picker("", selection: $searchMode) {
                            Text("Keyword").tag(SearchMode.keyword)
                            Text("QMD").tag(SearchMode.qmd)
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 140)
                        .onChange(of: searchMode) { _, _ in runSearch() }
                    }
                }
                .padding()
                if isSearchingQMD {
                    Text("Searching...")
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .padding(.horizontal)
                }
                if let err = qmdSearchError {
                    Text(err)
                        .font(.caption)
                        .foregroundColor(.red)
                        .padding(.horizontal)
                }
                List(displayedMeetings, id: \.id, selection: $selectedMeeting) { meeting in
                    NavigationLink(value: meeting) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(meeting.name)
                                .font(.headline)
                            Text(meeting.dateString)
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }
                }
                .listStyle(.sidebar)
            }
        } detail: {
            if let meeting = selectedMeeting {
                MeetingDetailView(meeting: meeting, refreshTrigger: $refreshTrigger, onRefresh: {
                    meetingStore.refresh()
                })
            } else {
                Text("Select a recording")
                    .foregroundColor(.secondary)
            }
        }
        .frame(minWidth: 500, minHeight: 400)
        .onAppear {
            meetingStore.refresh()
            syncLegacyFromFilesystem()
            qmdResults = meetingStore.meetings
        }
        .onChange(of: searchQuery) { _, _ in
            if searchMode == .qmd { runSearch() }
        }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Done") { isPresented = false }
            }
        }
    }

    /// Migrate legacy sessions (filesystem-only) into DB when DB is empty.
    private func syncLegacyFromFilesystem() {
        guard meetingStore.meetings.isEmpty else { return }
        var dirsToScan: [URL] = []
        if let rec = AppSettings.recordingsURL, FileManager.default.fileExists(atPath: rec.path) {
            dirsToScan.append(rec)
        }
        if let root = AppSettings.storageRootURL, FileManager.default.fileExists(atPath: root.path) {
            dirsToScan.append(root)
        }
        for base in dirsToScan {
            let contents = (try? FileManager.default.contentsOfDirectory(at: base, includingPropertiesForKeys: [.contentModificationDateKey], options: .skipsHiddenFiles)) ?? []
            for url in contents where url.hasDirectoryPath {
                let path = url.path
                guard let meeting = SessionMetadataParser.parse(recordingPath: path) else { continue }
                meetingStore.insertMeeting(meeting)
                if let content = SessionMetadataParser.readTranscript(recordingPath: path),
                   let tp = SessionMetadataParser.transcriptPath(fromRecordingPath: path) {
                    meetingStore.insertTranscript(meetingId: meeting.id, contentMd: content, transcriptPath: tp)
                }
            }
        }
    }
}

struct MeetingDetailView: View {
    let meeting: MeetingRecord
    @Binding var refreshTrigger: Int
    var onRefresh: () -> Void
    @State private var transcriptionProgress: UInt32 = 0
    @State private var isTranscribingThis = false
    @State private var transcribeError: String?
    @State private var pollTimer: Timer?

    private var transcriptContent: String? {
        MeetingStore.shared.transcript(forMeetingId: meeting.id)
            ?? SessionMetadataParser.readTranscript(recordingPath: meeting.recordingPath)
    }

    private var transcriptURL: URL? {
        (meeting.transcriptPath ?? SessionMetadataParser.transcriptPath(fromRecordingPath: meeting.recordingPath))
            .map { URL(fileURLWithPath: $0) }
    }

    private var hasAudio: Bool {
        let url = URL(fileURLWithPath: meeting.recordingPath)
        return FileManager.default.fileExists(atPath: url.appendingPathComponent("mic_000.wav").path)
            || FileManager.default.fileExists(atPath: url.appendingPathComponent("remote_000.wav").path)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(meeting.name)
                .font(.title2)
            Text(meeting.dateString)
                .font(.caption)
                .foregroundColor(.secondary)
            HStack {
                Button("Open Folder") {
                    NSWorkspace.shared.open(URL(fileURLWithPath: meeting.recordingPath))
                }
                if let url = transcriptURL, FileManager.default.fileExists(atPath: url.path) {
                    Button("Open Transcript") {
                        NSWorkspace.shared.open(url)
                    }
                }
                if !meeting.hasTranscript && hasAudio && !isTranscribingThis {
                    Button("Transcribe") {
                        transcribeError = nil
                        if let err = RustBridge.transcribeSession(path: meeting.recordingPath) {
                            transcribeError = err
                        } else {
                            isTranscribingThis = true
                        }
                    }
                }
            }
            if isTranscribingThis {
                Text("Transcribing... \(transcriptionProgress)%")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            if let err = transcribeError {
                Text(err)
                    .font(.caption)
                    .foregroundColor(.red)
            }
            if let content = transcriptContent {
                ScrollView {
                    Text(content)
                        .font(.system(.body, design: .monospaced))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                        .padding()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onAppear {
            pollTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
                if let path = RustBridge.transcribingSessionPath(), path == meeting.recordingPath {
                    transcriptionProgress = RustBridge.transcriptionProgress()
                } else if isTranscribingThis {
                    isTranscribingThis = false
                    onRefresh()
                }
            }
            RunLoop.main.add(pollTimer!, forMode: .common)
        }
        .onDisappear {
            pollTimer?.invalidate()
            pollTimer = nil
        }
    }
}
