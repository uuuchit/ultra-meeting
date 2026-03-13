# Ultra Meeting - Implementation Plan

## Project Overview

A reliable, local-first meeting recorder for macOS Intel that captures audio from any meeting app (Zoom, Google Meet, Slack Huddle, Teams, browser meetings) with automatic transcription.

**Core Philosophy**: Two-track recording (mic + remote audio) with app-level capture, post-meeting transcription, and markdown output.

---

## Architecture

```
┌─────────────────────────────────────┐
│   Menu Bar App (Swift/SwiftUI)     │
│   - UI & user controls              │
│   - Permission management           │
│   - Settings interface              │
└──────────────┬──────────────────────┘
               │ FFI Bridge
┌──────────────▼──────────────────────┐
│   Rust Core (tokio runtime)         │
│   - State machine                   │
│   - Audio orchestration             │
│   - Storage management              │
│   - Transcription queue             │
│   - Error recovery                  │
└──────────────┬──────────────────────┘
               │
    ┌──────────┴──────────┐
    │                     │
┌───▼────────┐   ┌────────▼─────────┐
│ cpal       │   │ Swift Bridge     │
│ (mic)      │   │ (ScreenCaptureKit│
└────────────┘   └──────────────────┘
```

---

## Phase 1: MVP Foundation

### Milestone 1.1: Project Setup & Permissions

**Goal**: Get basic Swift app running with proper permission handling

#### Tasks:
- [ ] Create Xcode project for menu bar app
- [ ] Set up Rust library with `cargo init --lib`
- [ ] Configure `swift-bridge` or C FFI for Rust ↔ Swift communication
- [ ] Add Info.plist entries:
  - `NSMicrophoneUsageDescription`
  - `NSScreenCaptureUsageDescription` (for ScreenCaptureKit audio)
- [ ] Implement permission checker:
  ```swift
  - Check microphone permission (AVCaptureDevice)
  - Check screen recording permission (CGPreflightScreenCaptureAccess)
  - Show permission prompt UI with instructions
  - Handle permission denial gracefully
  - Detect permission revocation during runtime
  ```
- [ ] Create menu bar UI:
  - App icon in menu bar
  - Status indicator (idle/recording/processing)
  - Start/Stop recording button
  - Settings menu item
  - Quit option

**Deliverable**: Menu bar app that checks permissions and shows status

---

### Milestone 1.2: Audio Capture - Microphone

**Goal**: Capture mic audio to WAV file

#### Tasks:
- [ ] Add Rust dependencies:
  ```toml
  cpal = "0.15"
  hound = "3.5"
  crossbeam = "0.8"
  tokio = { version = "1", features = ["full"] }
  tracing = "0.1"
  tracing-subscriber = "0.3"
  ```
- [ ] Implement mic capture module:
  - Enumerate audio input devices
  - Select default mic or let user choose
  - Configure stream: 48kHz, mono, f32 samples
  - Create lock-free ring buffer (crossbeam)
  - Audio callback: copy samples to ring buffer only
  - Worker thread: read from ring buffer, write to WAV
- [ ] Implement chunked WAV writer:
  - Write 5-second chunks
  - Use sequential naming: `mic_000.wav`, `mic_001.wav`, etc.
  - Flush after each chunk (crash safety)
- [ ] Add timestamp synchronization:
  - Use `mach_absolute_time()` for unified clock
  - Record start timestamp
  - Write timestamps to metadata file
- [ ] Error handling:
  - Device disconnection detection
  - Automatic device re-initialization
  - Disk space check before recording
  - Handle buffer overruns gracefully

**Deliverable**: Can record mic audio to chunked WAV files

---

### Milestone 1.3: Audio Capture - System/App Audio

**Goal**: Capture meeting app audio via ScreenCaptureKit

#### Tasks:
- [ ] Create Swift bridge for ScreenCaptureKit:
  - Enumerate available audio sources (apps/windows)
  - Start audio-only capture from target app
  - Stream audio samples to Rust via callback
  - Handle capture errors and report to Rust
- [ ] Implement app audio capture in Rust:
  - Receive samples from Swift bridge
  - Resample to 48kHz mono if needed (use `rubato`)
  - Write to separate ring buffer
  - Worker thread writes to `remote_000.wav`, `remote_001.wav`, etc.
- [ ] Sync both audio tracks:
  - Use same timestamp base for both streams
  - Detect clock drift every 10 seconds
  - Write sync markers to metadata
- [ ] Add app selection UI (Swift):
  - List running apps with audio
  - Let user select target app before recording
  - Remember last selection
- [ ] Fallback detection:
  - Detect when ScreenCaptureKit fails to capture audio
  - Log failure reason
  - Show user-friendly error message

**Deliverable**: Can record both mic and app audio as separate synchronized tracks

---

### Milestone 1.4: Storage & Metadata

**Goal**: Organize recordings with proper metadata

#### Tasks:
- [ ] Define storage structure:
  ```
  ~/Documents/UltraMeeting/
    recordings/
      2026-03-08_16-30-00_meeting-name/
        metadata.yaml
        mic_000.wav
        mic_001.wav
        remote_000.wav
        remote_001.wav
        transcript.md (generated later)
  ```
- [ ] Implement metadata format:
  ```yaml
  version: 1
  format: ultra-meeting-v1
  meeting:
    name: "Team Standup"
    start_time: "2026-03-08T16:30:00+05:30"
    end_time: "2026-03-08T17:00:00+05:30"
    duration_seconds: 1800
  audio:
    sample_rate: 48000
    channels: 1
    format: "16-bit PCM"
    mic_chunks: 360
    remote_chunks: 360
  sync:
    clock_base: "mach_absolute_time"
    start_timestamp: 1234567890
    drift_corrections: []
  capture:
    mic_device: "Built-in Microphone"
    remote_source: "zoom.us"
    remote_method: "ScreenCaptureKit"
  ```
- [ ] Create meeting folder on recording start
- [ ] Write metadata incrementally:
  - Initial metadata on start
  - Update on stop with end_time and duration
  - Append drift corrections during recording
- [ ] Add user input for meeting name:
  - Prompt on start or use default timestamp-based name
  - Sanitize filename (remove special chars)
- [ ] Implement storage location setting:
  - Default: `~/Documents/UltraMeeting/recordings/`
  - Let user change in settings
  - Validate path exists and is writable

**Deliverable**: Recordings organized in folders with complete metadata

---

### Milestone 1.5: State Machine & Error Recovery

**Goal**: Robust state management and crash recovery

#### Tasks:
- [ ] Define state machine:
  ```rust
  enum RecordingState {
      Idle,
      Preparing,      // Checking permissions, initializing devices
      Recording,      // Active capture
      Stopping,       // Flushing buffers
      Processing,     // Transcription in progress
      Error(String),  // Recoverable error
      Failed(String), // Unrecoverable error
  }
  ```
- [ ] Implement state transitions:
  - Idle → Preparing: User clicks start
  - Preparing → Recording: Devices initialized
  - Preparing → Error: Device init failed
  - Recording → Stopping: User clicks stop
  - Recording → Error: Device disconnected, disk full
  - Stopping → Processing: Buffers flushed
  - Processing → Idle: Transcription complete
  - Error → Idle: User acknowledges error
  - Error → Recording: Auto-recovery succeeded
- [ ] Add state persistence:
  - Write current state to `~/.ultra-meeting/state.json`
  - On app launch, check for interrupted recording
  - Offer to resume or discard
- [ ] Implement error recovery:
  - Device disconnection: try to reconnect every 2s
  - Disk full: stop recording, notify user
  - Process crash: mark recording as incomplete in metadata
  - System sleep: pause recording, resume on wake
- [ ] Add logging:
  ```rust
  use tracing::{info, warn, error, debug};
  
  // Log all state transitions
  // Log device events
  // Log errors with full context
  // Write logs to ~/.ultra-meeting/logs/
  ```

**Deliverable**: Robust recording with error recovery and crash safety

---

### Milestone 1.6: Post-Meeting Transcription

**Goal**: Automatic transcription with progress tracking

#### Tasks:
- [ ] Add transcription dependencies:
  ```toml
  whisper-rs = "0.10"
  ```
- [ ] Download Whisper model:
  - Use `base.en` for Intel Mac
  - Store in `~/.ultra-meeting/models/`
  - Download on first run or via settings
- [ ] Implement VAD (Voice Activity Detection):
  - Use `webrtc-vad` or simple energy-based VAD
  - Scan both audio tracks
  - Create segments: `[(start_time, end_time, track)]`
  - Skip silence (energy below threshold)
- [ ] Implement transcription pipeline:
  - Merge WAV chunks into single file per track
  - Run VAD to get speech segments
  - Transcribe each segment with Whisper
  - Combine results with timestamps
- [ ] Create transcript format:
  ```markdown
  ---
  version: 1
  meeting: Team Standup
  date: 2026-03-08
  duration: 30m 15s
  participants:
    - You (mic)
    - Remote
  ---
  
  # Team Standup
  
  **Date**: March 8, 2026  
  **Duration**: 30 minutes 15 seconds
  
  ## Transcript
  
  **[00:00:05] You**: Hey everyone, let's get started...
  
  **[00:00:12] Remote**: Sounds good, I'll go first...
  
  **[00:01:45] You**: What about the API integration?
  ```
- [ ] Add progress tracking:
  - Show progress in menu bar (percentage)
  - Allow cancellation
  - Resume from last transcribed segment if cancelled
- [ ] Run transcription in background:
  - Use tokio task
  - Low priority (don't block UI)
  - Throttle CPU usage if needed
- [ ] Handle transcription errors:
  - Model loading failure
  - Out of memory
  - Corrupted audio file
  - Log error and mark transcript as incomplete

**Deliverable**: Automatic transcription with progress indicator and markdown output

---

### Milestone 1.7: MVP Polish

**Goal**: Usable product for daily use

#### Tasks:
- [ ] Improve menu bar UI:
  - Show recording duration in real-time
  - Add "Open Recordings Folder" menu item
  - Add "View Last Recording" menu item
  - Show transcription progress
- [ ] Add keyboard shortcuts:
  - Global hotkey to start/stop recording (optional, needs accessibility permission)
  - Or menu bar shortcuts
- [ ] Implement settings panel:
  - Storage location
  - Default meeting name prefix
  - Mic device selection
  - Whisper model selection
  - Enable/disable auto-transcription
- [ ] Add notifications:
  - Recording started
  - Recording stopped
  - Transcription complete
  - Errors (with actionable message)
- [ ] Create app icon and menu bar icon
- [ ] Write README with:
  - Installation instructions
  - Permission setup guide
  - Usage guide
  - Troubleshooting
- [ ] Test on Intel Mac:
  - Zoom meeting
  - Google Meet (browser)
  - Slack Huddle
  - Long meeting (1+ hour) for drift testing
  - Device disconnection during recording
  - Disk full scenario
  - App crash and recovery

**Deliverable**: Polished MVP ready for personal use

---

## Phase 2: Enhanced Features

### Milestone 2.1: Audio Sync Refinement

**Goal**: Perfect sync for long meetings

#### Tasks:
- [ ] Implement drift detection:
  - Compare timestamps every 10 seconds
  - Calculate drift rate
  - Log drift corrections to metadata
- [ ] Add post-recording alignment:
  - Analyze both tracks for common audio patterns
  - Use cross-correlation to detect offset
  - Generate alignment report
  - Optionally re-align tracks
- [ ] Add test tone at recording start:
  - Play brief tone (1kHz, 100ms) through speakers
  - Capture in both mic and remote tracks
  - Use for precise alignment verification

**Deliverable**: Verified sync accuracy for 2+ hour meetings

---

### Milestone 2.2: Auto-Detection for Zoom

**Goal**: Automatic recording when Zoom meeting starts

#### Tasks:
- [ ] Implement Zoom detection:
  - Monitor running processes for `zoom.us`
  - Use Accessibility API to detect meeting window
  - Parse window title for meeting name
  - Detect meeting start/end via window state changes
- [ ] Add auto-start setting:
  - Enable/disable auto-recording
  - Confirmation prompt before starting
  - Whitelist/blacklist meeting patterns
- [ ] Handle edge cases:
  - Multiple Zoom windows
  - Screen sharing vs meeting window
  - Zoom webinar vs meeting

**Deliverable**: Automatic recording for Zoom meetings

---

### Milestone 2.3: Speaker Diarization

**Goal**: Label "You" vs "Remote" speakers accurately

#### Tasks:
- [ ] Implement simple diarization:
  - Analyze energy levels in both tracks
  - When mic track has speech, label as "You"
  - When remote track has speech, label as "Remote"
  - Handle overlapping speech
- [ ] Improve with ML (optional):
  - Use pyannote.audio or similar
  - Distinguish multiple remote speakers
  - Label as "Remote 1", "Remote 2", etc.
- [ ] Update transcript format with speaker labels

**Deliverable**: Accurate speaker labeling in transcripts

---

### Milestone 2.4: Search & Browse

**Goal**: Find past meetings easily

#### Tasks:
- [ ] Add meeting list view:
  - Show all recordings sorted by date
  - Display meeting name, date, duration
  - Search by name or date
  - Filter by date range
- [ ] Implement basic search:
  - Grep-style search across transcripts
  - Show matching meetings with context
  - Highlight search terms
- [ ] Add meeting details view:
  - Show metadata
  - Display transcript
  - Play audio (optional)
  - Export options

**Deliverable**: Browse and search past meetings

---

### Milestone 2.5: Summaries & Action Items

**Goal**: Automatic meeting summaries

#### Tasks:
- [ ] Integrate LLM for summarization:
  - Use local model (llama.cpp) or API (OpenAI, Anthropic)
  - Generate summary from transcript
  - Extract action items
  - Identify key decisions
- [ ] Add summary to transcript:
  ```markdown
  ## Summary
  
  Brief overview of meeting...
  
  ## Action Items
  
  - [ ] @You: Complete API integration by Friday
  - [ ] @Remote: Review PR #123
  
  ## Key Decisions
  
  - Decided to use Rust for backend
  - Postponed UI redesign to next sprint
  ```
- [ ] Make summarization optional (privacy-conscious users)

**Deliverable**: Automatic meeting summaries and action items

---

## Phase 3: Advanced Features

### Milestone 3.1: BlackHole Fallback

**Goal**: Capture system audio when app-level capture fails

#### Tasks:
- [ ] Detect ScreenCaptureKit failure
- [ ] Guide user to install BlackHole
- [ ] Configure Multi-Output Device programmatically
- [ ] Switch to BlackHole capture
- [ ] Restore original audio output after recording

**Deliverable**: Fallback for apps where ScreenCaptureKit doesn't work

---

### Milestone 3.2: Browser Extension for Google Meet

**Goal**: Precise detection for browser meetings

#### Tasks:
- [ ] Create Chrome extension:
  - Detect meet.google.com
  - Extract meeting name from page
  - Send start/stop signals to app via native messaging
- [ ] Implement native messaging host
- [ ] Add Safari extension (if needed)

**Deliverable**: Auto-detection for Google Meet

---

### Milestone 3.3: Encryption & Privacy

**Goal**: Secure local storage

#### Tasks:
- [ ] Implement encryption:
  - Use user's keychain for encryption key
  - Encrypt audio files at rest
  - Encrypt transcripts
- [ ] Add privacy settings:
  - Exclude from Spotlight index
  - Exclude from Time Machine (optional)
  - Auto-delete after N days
  - Secure delete (overwrite)

**Deliverable**: Encrypted local archive

---

### Milestone 3.4: Full-Text Search Index

**Goal**: Fast search across all meetings

#### Tasks:
- [ ] Implement search index:
  - Use tantivy or similar
  - Index all transcripts
  - Update index on new recordings
- [ ] Add advanced search:
  - Full-text search
  - Date range filters
  - Speaker filters
  - Duration filters

**Deliverable**: Fast full-text search

---

## Technical Specifications

### Audio Format Standards

- **Sample Rate**: 48 kHz (industry standard for video conferencing)
- **Channels**: Mono (separate tracks for mic and remote)
- **Bit Depth**: 16-bit PCM (good quality, reasonable size)
- **Chunk Size**: 5 seconds (~480 KB per chunk per track)
- **Total Size**: ~11.5 MB per minute for both tracks

### File Naming Conventions

- **Meeting Folder**: `YYYY-MM-DD_HH-MM-SS_meeting-name/`
- **Audio Chunks**: `mic_NNN.wav`, `remote_NNN.wav` (zero-padded)
- **Metadata**: `metadata.yaml`
- **Transcript**: `transcript.md`
- **Logs**: `recording.log`

### Error Codes

```rust
enum RecordingError {
    PermissionDenied(String),
    DeviceNotFound(String),
    DeviceDisconnected(String),
    DiskFull,
    BufferOverrun,
    TranscriptionFailed(String),
    InvalidState(String),
}
```

### Performance Targets

- **CPU Usage**: < 5% during recording (Intel Mac)
- **Memory Usage**: < 100 MB during recording
- **Transcription Speed**: Real-time factor < 0.3 (1 hour audio in < 18 minutes)
- **Startup Time**: < 2 seconds
- **UI Responsiveness**: < 100ms for all interactions

---

## Development Workflow

### Setup

1. Install Xcode and Xcode Command Line Tools
2. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
3. Install dependencies: `brew install blackhole-2ch` (for testing fallback)
4. Clone project and open in Xcode

### Build

```bash
# Build Rust library
cd rust-core
cargo build --release

# Build Swift app in Xcode
# Link against libultra_meeting.a
```

### Testing

- Unit tests for Rust modules
- Integration tests for audio capture
- Manual testing with real meetings
- Test on different macOS versions (10.15+)

### Logging

- Development: `RUST_LOG=debug`
- Production: `RUST_LOG=info`
- Logs location: `~/.ultra-meeting/logs/`

---

## Dependencies

### Rust

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
cpal = "0.15"
hound = "3.5"
rubato = "0.14"
crossbeam = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
whisper-rs = "0.10"
anyhow = "1"
thiserror = "1"
```

### Swift

- SwiftUI (UI framework)
- ScreenCaptureKit (audio capture)
- AVFoundation (permission checking)
- UserNotifications (notifications)

---

## Security Considerations

1. **Permissions**: Request only necessary permissions, explain why
2. **Storage**: Store recordings in user-accessible location
3. **Encryption**: Optional, user-controlled
4. **Network**: No network access by default (fully local)
5. **Privacy**: No telemetry, no analytics, no cloud sync
6. **Sandboxing**: Use App Sandbox with minimal entitlements

---

## Distribution

### MVP

- Direct download (DMG)
- Manual installation
- No code signing initially (development)

### Future

- Code sign with Apple Developer ID
- Notarization for Gatekeeper
- Homebrew cask
- Possible Mac App Store (requires sandboxing adjustments)

---

## Success Criteria

### MVP Success

- [ ] Records Zoom meetings reliably
- [ ] Records Google Meet (browser) reliably
- [ ] Transcription accuracy > 90% for clear audio
- [ ] No crashes during 1-hour meeting
- [ ] Sync drift < 100ms over 1 hour
- [ ] User can find and read past transcripts easily

### Phase 2 Success

- [ ] Auto-detects Zoom meetings
- [ ] Speaker labels are 95%+ accurate
- [ ] Search finds relevant meetings quickly
- [ ] Summaries are useful and accurate

### Phase 3 Success

- [ ] Works with all major meeting apps
- [ ] Encrypted storage available
- [ ] Fast full-text search across 100+ meetings

---

## Timeline Estimate

- **Phase 1 (MVP)**: 6-8 weeks (full-time) or 3-4 months (part-time)
- **Phase 2**: 4-6 weeks
- **Phase 3**: 4-6 weeks

**Total**: 4-6 months for complete product

---

## Next Steps

1. Set up Xcode project
2. Create Rust library structure
3. Implement permission checking (Milestone 1.1)
4. Start with mic capture (Milestone 1.2)

Ready to start coding!
