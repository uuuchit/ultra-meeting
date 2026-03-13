import Foundation

/// User preferences stored in UserDefaults.
struct AppSettings {
    private static let storageKey = "storagePath"
    private static let meetingNameKey = "defaultMeetingName"
    private static let hasCompletedOnboardingKey = "hasCompletedOnboarding"
    private static let qmdEnabledKey = "qmdSearchEnabled"
    
    static var storagePath: String? {
        get { UserDefaults.standard.string(forKey: storageKey) }
        set { UserDefaults.standard.set(newValue, forKey: storageKey) }
    }
    
    static var defaultMeetingName: String {
        get { UserDefaults.standard.string(forKey: meetingNameKey) ?? "Meeting" }
        set { UserDefaults.standard.set(newValue, forKey: meetingNameKey) }
    }
    
    static var hasCompletedOnboarding: Bool {
        get { UserDefaults.standard.bool(forKey: hasCompletedOnboardingKey) }
        set { UserDefaults.standard.set(newValue, forKey: hasCompletedOnboardingKey) }
    }

    static var qmdSearchEnabled: Bool {
        get { UserDefaults.standard.bool(forKey: qmdEnabledKey) }
        set { UserDefaults.standard.set(newValue, forKey: qmdEnabledKey) }
    }
    
    /// Base storage root. Recordings and transcripts are in subfolders.
    static var storageRootURL: URL? {
        if let path = storagePath, !path.isEmpty {
            return URL(fileURLWithPath: path)
        }
        return FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first?
            .appendingPathComponent("UltraMeeting")
    }

    /// Recordings folder: {storage_root}/recordings/
    static var recordingsURL: URL? {
        storageRootURL?.appendingPathComponent("recordings")
    }

    /// Transcripts folder: {storage_root}/transcripts/
    static var transcriptsURL: URL? {
        storageRootURL?.appendingPathComponent("transcripts")
    }
    
    static func lastRecordingURL() -> URL? {
        guard let base = recordingsURL, FileManager.default.fileExists(atPath: base.path) else { return nil }
        let contents = (try? FileManager.default.contentsOfDirectory(at: base, includingPropertiesForKeys: [.contentModificationDateKey], options: .skipsHiddenFiles)) ?? []
        return contents
            .filter { $0.hasDirectoryPath }
            .max(by: { a, b in (try? a.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate ?? .distantPast) ?? .distantPast < (try? b.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate ?? .distantPast) ?? .distantPast })
    }
}
