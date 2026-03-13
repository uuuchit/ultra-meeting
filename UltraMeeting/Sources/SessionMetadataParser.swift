import Foundation

/// Parses metadata.yaml from a recording session folder.
enum SessionMetadataParser {
    /// Parse metadata.yaml and return MeetingRecord components.
    /// recordingPath: path to recordings/{id}/ folder.
    static func parse(recordingPath: String) -> MeetingRecord? {
        let metaURL = URL(fileURLWithPath: recordingPath).appendingPathComponent("metadata.yaml")
        guard let yaml = try? String(contentsOf: metaURL) else { return nil }
        return parseYAML(yaml, recordingPath: recordingPath)
    }

    private static func parseYAML(_ yaml: String, recordingPath: String) -> MeetingRecord? {
        var name = "Meeting"
        var startTime = ""
        var endTime: String?
        var durationSecs: Int?
        var micChunks = 0
        var remoteChunks = 0
        var inMeeting = false
        var inAudio = false

        for line in yaml.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            let indent = line.prefix(while: { $0 == " " || $0 == "\t" }).count

            if indent == 0 {
                inMeeting = trimmed == "meeting:"
                inAudio = trimmed == "audio:"
                if !inMeeting && !inAudio { inMeeting = false; inAudio = false }
                continue
            }

            if inMeeting {
                if trimmed.hasPrefix("name:") {
                    name = trimmed.dropFirst(5).trimmingCharacters(in: .whitespaces)
                } else if trimmed.hasPrefix("start_time:") {
                    startTime = trimmed.dropFirst(11).trimmingCharacters(in: .whitespaces)
                } else if trimmed.hasPrefix("end_time:") {
                    let val = trimmed.dropFirst(9).trimmingCharacters(in: .whitespaces)
                    endTime = val.isEmpty ? nil : val
                } else if trimmed.hasPrefix("duration_seconds:") {
                    durationSecs = Int(trimmed.dropFirst(17).trimmingCharacters(in: .whitespaces))
                }
            }
            if inAudio {
                if trimmed.hasPrefix("mic_chunks:") {
                    micChunks = Int(trimmed.dropFirst(11).trimmingCharacters(in: .whitespaces)) ?? 0
                } else if trimmed.hasPrefix("remote_chunks:") {
                    remoteChunks = Int(trimmed.dropFirst(13).trimmingCharacters(in: .whitespaces)) ?? 0
                }
            }
        }

        guard !startTime.isEmpty else { return nil }
        let sessionId = (recordingPath as NSString).lastPathComponent
        let tp = transcriptPath(fromRecordingPath: recordingPath)
        let transcriptPathValue: String? = tp.flatMap { FileManager.default.fileExists(atPath: $0) ? $0 : nil }
        let now = ISO8601DateFormatter().string(from: Date())
        return MeetingRecord(
            id: sessionId,
            name: name,
            recordingPath: recordingPath,
            transcriptPath: transcriptPathValue,
            startTime: startTime,
            endTime: endTime,
            durationSecs: durationSecs,
            micChunks: micChunks,
            remoteChunks: remoteChunks,
            createdAt: now
        )
    }

    /// Derive transcript path from recording path.
    /// Legacy: transcript.md inside session folder.
    /// New layout: recordings/{id}/ -> transcripts/{id}/transcript.md
    static func transcriptPath(fromRecordingPath recordingPath: String) -> String? {
        let url = URL(fileURLWithPath: recordingPath)
        let legacyPath = url.appendingPathComponent("transcript.md").path
        if FileManager.default.fileExists(atPath: legacyPath) { return legacyPath }
        let sessionId = url.lastPathComponent
        let parent = url.deletingLastPathComponent()
        let storageBase = parent.lastPathComponent == "recordings" ? parent.deletingLastPathComponent() : parent
        let transcriptURL = storageBase
            .appendingPathComponent("transcripts")
            .appendingPathComponent(sessionId)
            .appendingPathComponent("transcript.md")
        return transcriptURL.path
    }

    /// Read transcript content from transcript file (new or legacy layout).
    static func readTranscript(recordingPath: String) -> String? {
        let path = transcriptPath(fromRecordingPath: recordingPath)
            ?? URL(fileURLWithPath: recordingPath).appendingPathComponent("transcript.md").path
        return try? String(contentsOf: URL(fileURLWithPath: path))
    }
}
