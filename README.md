# Ultra Meeting

Local-first meeting recorder for macOS: two-track audio capture (mic + remote) with automatic transcription.

## Requirements

- macOS 15+
- Xcode 16+
- Rust (for rust-core)
- cmake (optional, for transcription feature)

## Installation

### First Time Setup

```bash
./install.sh
```

This will:
- Build the Rust core and Swift app
- Install to `/Applications/Ultra Meeting.app`
- Code sign the app
- Launch it automatically

The app will appear in your menu bar. Grant microphone and screen recording permissions when prompted.

### Development Updates

After making code changes:

```bash
./update.sh
```

This quickly rebuilds and updates the installed app without requiring new permissions.

## Permissions

On first run, grant:
1. **Microphone** – for your voice
2. **Screen Recording** – for meeting app audio (Zoom, Meet, etc.)

**Important:** Permissions are tied to the bundle ID (`com.ultrameeting.app`). Once granted, they persist across builds and updates. No more duplicate entries in System Settings!

Note: After granting screen capture, the app may need to restart for the permission to take effect.

## Usage

1. Click the menu bar icon
2. Acknowledge the recording disclosure
3. Grant microphone and screen capture if prompted
4. Choose target app for remote audio (future)
5. Click Start Recording
6. Click Stop when done
7. Transcript appears in `~/Documents/UltraMeeting/recordings/`

## Project Structure

```
ultra-meeting/
  plan.md              # Original implementation plan
  rust-core/           # State machine, capture, storage
  UltraMeeting/        # Swift menu bar app
  spikes/              # Phase 0 validation spikes
  docs/                # Architecture, test matrix, release
```

## Phase 0 Spikes

Run technical validation before main development:

```bash
./spikes/run-spikes.sh
```

## License

MIT
