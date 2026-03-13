import Foundation

struct QMDError: Error {
    let message: String
}

/// Wrapper for QMD CLI (https://github.com/tobi/qmd) for semantic + keyword search.
/// Requires: npm install -g @tobilu/qmd
final class QMDService {
    static let shared = QMDService()
    static let collectionName = "ultra-meeting-transcripts"

    private init() {}

    func isQMDInstalled() -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["which", "qmd"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            return process.terminationStatus == 0 && !data.isEmpty
        } catch {
            return false
        }
    }

    func addTranscriptsCollection(path: String) -> Result<Void, QMDError> {
        runQMD(["collection", "add", path, "--name", Self.collectionName])
    }

    /// Add context to improve search relevance. Run after addTranscriptsCollection.
    func addContext() -> Result<Void, QMDError> {
        runQMD(["context", "add", "qmd://\(Self.collectionName)", "Meeting transcripts from UltraMeeting"])
    }

    func updateIndex() -> Result<Void, QMDError> {
        runQMD(["update"])
    }

    func embed() -> Result<Void, QMDError> {
        runQMD(["embed"])
    }

    /// Keyword search (BM25). Returns file paths.
    func search(query: String, limit: Int = 20) -> Result<[QMDSearchResult], QMDError> {
        runQMDSearch(["search", query, "-c", Self.collectionName, "--json", "-n", "\(limit)"])
    }

    /// Hybrid/semantic search with LLM reranking.
    func query(query: String, limit: Int = 20) -> Result<[QMDSearchResult], QMDError> {
        runQMDSearch(["query", query, "-c", Self.collectionName, "--json", "-n", "\(limit)"])
    }

    private func runQMD(_ args: [String]) -> Result<Void, QMDError> {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["qmd"] + args
        let pipe = Pipe()
        let errPipe = Pipe()
        process.standardOutput = pipe
        process.standardError = errPipe
        do {
            try process.run()
            process.waitUntilExit()
            if process.terminationStatus != 0 {
                let err = String(data: errPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "Unknown error"
                return .failure(QMDError(message: err))
            }
            return .success(())
        } catch {
            return .failure(QMDError(message: error.localizedDescription))
        }
    }

    private func runQMDSearch(_ args: [String]) -> Result<[QMDSearchResult], QMDError> {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["qmd"] + args
        let pipe = Pipe()
        let errPipe = Pipe()
        process.standardOutput = pipe
        process.standardError = errPipe
        do {
            try process.run()
            process.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            if process.terminationStatus != 0 {
                let err = String(data: errPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "Unknown error"
                return .failure(QMDError(message: err))
            }
            guard let json = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
                return .success([])
            }
            let results = json.compactMap { QMDSearchResult.from(dict: $0) }
            return .success(results)
        } catch {
            return .failure(QMDError(message: error.localizedDescription))
        }
    }
}

struct QMDSearchResult {
    let path: String
    let score: Double?
    let snippet: String?

    static func from(dict: [String: Any]) -> QMDSearchResult? {
        guard let path = dict["path"] as? String ?? dict["file"] as? String else { return nil }
        let score = (dict["score"] as? NSNumber)?.doubleValue
        let snippet = dict["snippet"] as? String ?? dict["text"] as? String
        return QMDSearchResult(path: path, score: score, snippet: snippet)
    }
}
