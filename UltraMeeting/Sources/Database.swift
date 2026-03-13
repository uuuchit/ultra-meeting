import Foundation
import SQLite3

private let SQLITE_TRANSIENT = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

/// SQLite database for meetings and transcripts.
/// Location: ~/Library/Application Support/UltraMeeting/ultra_meeting.db
final class Database {
    private var db: OpaquePointer?
    private let path: String

    static let schemaVersion: Int32 = 1

    init?(path: String? = nil) {
        if let p = path {
            self.path = p
        } else if let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
            let ultraDir = appSupport.appendingPathComponent("UltraMeeting")
            try? FileManager.default.createDirectory(at: ultraDir, withIntermediateDirectories: true)
            self.path = ultraDir.appendingPathComponent("ultra_meeting.db").path
        } else {
            return nil
        }
        guard open() else { return nil }
    }

    deinit {
        sqlite3_close(db)
    }

    private func open() -> Bool {
        guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE, nil) == SQLITE_OK else {
            return false
        }
        return migrate()
    }

    private func migrate() -> Bool {
        let current = userVersion()
        if current == 0 {
            return createSchema()
        }
        // Future migrations go here
        return true
    }

    private func userVersion() -> Int32 {
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, "PRAGMA user_version", -1, &stmt, nil) == SQLITE_OK,
              sqlite3_step(stmt) == SQLITE_ROW else {
            return 0
        }
        return sqlite3_column_int(stmt, 0)
    }

    private func createSchema() -> Bool {
        let sql = """
        CREATE TABLE IF NOT EXISTS meetings (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            recording_path TEXT NOT NULL,
            transcript_path TEXT,
            start_time TEXT NOT NULL,
            end_time TEXT,
            duration_secs INTEGER,
            mic_chunks INTEGER DEFAULT 0,
            remote_chunks INTEGER DEFAULT 0,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS transcripts (
            meeting_id TEXT PRIMARY KEY REFERENCES meetings(id),
            content_md TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(
            meeting_id,
            content_md,
            content='transcripts',
            content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS transcripts_fts_insert AFTER INSERT ON transcripts BEGIN
            INSERT INTO transcripts_fts(rowid, meeting_id, content_md) VALUES (new.rowid, new.meeting_id, new.content_md);
        END;
        CREATE TRIGGER IF NOT EXISTS transcripts_fts_update AFTER UPDATE ON transcripts BEGIN
            INSERT INTO transcripts_fts(transcripts_fts, rowid, meeting_id, content_md) VALUES ('delete', old.rowid, old.meeting_id, old.content_md);
            INSERT INTO transcripts_fts(rowid, meeting_id, content_md) VALUES (new.rowid, new.meeting_id, new.content_md);
        END;
        CREATE TRIGGER IF NOT EXISTS transcripts_fts_delete AFTER DELETE ON transcripts BEGIN
            INSERT INTO transcripts_fts(transcripts_fts, rowid, meeting_id, content_md) VALUES ('delete', old.rowid, old.meeting_id, old.content_md);
        END;

        PRAGMA user_version = \(Database.schemaVersion);
        """
        var err: UnsafeMutablePointer<CChar>?
        guard sqlite3_exec(db, sql, nil, nil, &err) == SQLITE_OK else {
            if let e = err {
                NSLog("Database schema error: %s", e)
                sqlite3_free(e)
            }
            return false
        }
        return true
    }

    func insertMeeting(_ m: MeetingRecord) -> Bool {
        let sql = """
        INSERT OR REPLACE INTO meetings (id, name, recording_path, transcript_path, start_time, end_time, duration_secs, mic_chunks, remote_chunks, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return false }
        m.id.withCString { sqlite3_bind_text(stmt, 1, $0, -1, SQLITE_TRANSIENT) }
        m.name.withCString { sqlite3_bind_text(stmt, 2, $0, -1, SQLITE_TRANSIENT) }
        m.recordingPath.withCString { sqlite3_bind_text(stmt, 3, $0, -1, SQLITE_TRANSIENT) }
        if let tp = m.transcriptPath {
            tp.withCString { sqlite3_bind_text(stmt, 4, $0, -1, SQLITE_TRANSIENT) }
        } else {
            sqlite3_bind_null(stmt, 4)
        }
        m.startTime.withCString { sqlite3_bind_text(stmt, 5, $0, -1, SQLITE_TRANSIENT) }
        if let et = m.endTime {
            et.withCString { sqlite3_bind_text(stmt, 6, $0, -1, SQLITE_TRANSIENT) }
        } else {
            sqlite3_bind_null(stmt, 6)
        }
        sqlite3_bind_int(stmt, 7, Int32(m.durationSecs ?? 0))
        sqlite3_bind_int(stmt, 8, Int32(m.micChunks))
        sqlite3_bind_int(stmt, 9, Int32(m.remoteChunks))
        m.createdAt.withCString { sqlite3_bind_text(stmt, 10, $0, -1, SQLITE_TRANSIENT) }
        return sqlite3_step(stmt) == SQLITE_DONE
    }

    func updateMeetingTranscriptPath(meetingId: String, transcriptPath: String) -> Bool {
        let sql = "UPDATE meetings SET transcript_path = ? WHERE id = ?"
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return false }
        transcriptPath.withCString { sqlite3_bind_text(stmt, 1, $0, -1, SQLITE_TRANSIENT) }
        meetingId.withCString { sqlite3_bind_text(stmt, 2, $0, -1, SQLITE_TRANSIENT) }
        return sqlite3_step(stmt) == SQLITE_DONE
    }

    func insertTranscript(meetingId: String, contentMd: String) -> Bool {
        let now = ISO8601DateFormatter().string(from: Date())
        let sql = "INSERT OR REPLACE INTO transcripts (meeting_id, content_md, created_at) VALUES (?, ?, ?)"
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return false }
        meetingId.withCString { sqlite3_bind_text(stmt, 1, $0, -1, SQLITE_TRANSIENT) }
        contentMd.withCString { sqlite3_bind_text(stmt, 2, $0, -1, SQLITE_TRANSIENT) }
        now.withCString { sqlite3_bind_text(stmt, 3, $0, -1, SQLITE_TRANSIENT) }
        return sqlite3_step(stmt) == SQLITE_DONE
    }

    func recentMeetings(limit: Int = 10) -> [MeetingRecord] {
        let sql = "SELECT id, name, recording_path, transcript_path, start_time, end_time, duration_secs, mic_chunks, remote_chunks, created_at FROM meetings ORDER BY created_at DESC LIMIT ?"
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return [] }
        sqlite3_bind_int(stmt, 1, Int32(limit))
        var result: [MeetingRecord] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            if let m = rowToMeeting(stmt) {
                result.append(m)
            }
        }
        return result
    }

    func allMeetings() -> [MeetingRecord] {
        let sql = "SELECT id, name, recording_path, transcript_path, start_time, end_time, duration_secs, mic_chunks, remote_chunks, created_at FROM meetings ORDER BY created_at DESC"
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return [] }
        var result: [MeetingRecord] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            if let m = rowToMeeting(stmt) {
                result.append(m)
            }
        }
        return result
    }

    func transcript(forMeetingId id: String) -> String? {
        let sql = "SELECT content_md FROM transcripts WHERE meeting_id = ?"
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return nil }
        id.withCString { sqlite3_bind_text(stmt, 1, $0, -1, SQLITE_TRANSIENT) }
        guard sqlite3_step(stmt) == SQLITE_ROW else { return nil }
        return String(cString: sqlite3_column_text(stmt, 0))
    }

    func searchTranscripts(query: String) -> [MeetingRecord] {
        let sql = """
        SELECT m.id, m.name, m.recording_path, m.transcript_path, m.start_time, m.end_time, m.duration_secs, m.mic_chunks, m.remote_chunks, m.created_at
        FROM meetings m
        JOIN transcripts_fts f ON f.meeting_id = m.id
        WHERE f MATCH ?
        ORDER BY rank
        """
        var stmt: OpaquePointer?
        defer { sqlite3_finalize(stmt) }
        let escaped = query.replacingOccurrences(of: "\"", with: "\"\"")
        let match = "\"\(escaped)\""
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return [] }
        match.withCString { sqlite3_bind_text(stmt, 1, $0, -1, SQLITE_TRANSIENT) }
        var result: [MeetingRecord] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            if let m = rowToMeeting(stmt) {
                result.append(m)
            }
        }
        return result
    }

    private func rowToMeeting(_ stmt: OpaquePointer?) -> MeetingRecord? {
        guard let stmt else { return nil }
        let id = String(cString: sqlite3_column_text(stmt, 0))
        let name = String(cString: sqlite3_column_text(stmt, 1))
        let recordingPath = String(cString: sqlite3_column_text(stmt, 2))
        let transcriptPath = sqlite3_column_text(stmt, 3).map { String(cString: $0) }
        let startTime = String(cString: sqlite3_column_text(stmt, 4))
        let endTime = sqlite3_column_text(stmt, 5).map { String(cString: $0) }
        let durationSecs = Int(sqlite3_column_int(stmt, 6))
        let micChunks = Int(sqlite3_column_int(stmt, 7))
        let remoteChunks = Int(sqlite3_column_int(stmt, 8))
        let createdAt = String(cString: sqlite3_column_text(stmt, 9))
        return MeetingRecord(
            id: id,
            name: name,
            recordingPath: recordingPath,
            transcriptPath: transcriptPath,
            startTime: startTime,
            endTime: endTime,
            durationSecs: durationSecs,
            micChunks: micChunks,
            remoteChunks: remoteChunks,
            createdAt: createdAt
        )
    }
}

struct MeetingRecord: Hashable {
    let id: String
    let name: String
    let recordingPath: String
    let transcriptPath: String?
    let startTime: String
    let endTime: String?
    let durationSecs: Int?
    let micChunks: Int
    let remoteChunks: Int
    let createdAt: String

    var date: Date? {
        ISO8601DateFormatter().date(from: startTime)
    }

    var dateString: String {
        guard let d = date else { return startTime }
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .short
        return f.string(from: d)
    }

    var hasTranscript: Bool { transcriptPath != nil }
}
