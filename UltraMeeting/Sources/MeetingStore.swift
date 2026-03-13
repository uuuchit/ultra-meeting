import Foundation

/// Central store for meetings and transcripts. Uses Database and provides observable state.
final class MeetingStore: ObservableObject {
    static let shared = MeetingStore()

    @Published private(set) var meetings: [MeetingRecord] = []
    @Published private(set) var recentMeetings: [MeetingRecord] = []

    private let db: Database?
    private let recentLimit = 5

    init(db: Database? = Database()) {
        self.db = db
        refresh()
    }

    func refresh() {
        guard let db else { return }
        meetings = db.allMeetings()
        recentMeetings = db.recentMeetings(limit: recentLimit)
    }

    func insertMeeting(_ m: MeetingRecord) {
        guard let db, db.insertMeeting(m) else { return }
        refresh()
    }

    func insertTranscript(meetingId: String, contentMd: String, transcriptPath: String? = nil) {
        guard let db, db.insertTranscript(meetingId: meetingId, contentMd: contentMd) else { return }
        if let tp = transcriptPath {
            _ = db.updateMeetingTranscriptPath(meetingId: meetingId, transcriptPath: tp)
        }
        refresh()
    }

    func transcript(forMeetingId id: String) -> String? {
        db?.transcript(forMeetingId: id)
    }

    func searchTranscripts(query: String) -> [MeetingRecord] {
        guard let db, !query.trimmingCharacters(in: .whitespaces).isEmpty else {
            return meetings
        }
        let results = db.searchTranscripts(query: query)
        return results.isEmpty ? meetings : results
    }
}
