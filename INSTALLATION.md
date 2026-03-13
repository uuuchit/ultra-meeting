# Ultra Meeting - Installation & Development Guide

## Problem Solved

Previously, after each code change and build, you had to:
- Grant permissions again
- Deal with multiple "Ultra Meeting" entries in System Settings
- Manually manage the app location

Now the app behaves like a proper macOS application with persistent permissions.

## What Changed

1. **Added Bundle Identifier**: `com.ultrameeting.app` in Info.plist
2. **Added Version Info**: CFBundleVersion and CFBundleShortVersionString
3. **Added LSUIElement**: Makes it a proper menu bar app
4. **Installation to /Applications**: App now lives in the standard location
5. **Persistent Permissions**: Tied to bundle ID, not app location

## Installation

### First Time Setup

```bash
cd ~/Desktop/Projects/ultra-meeting
./install.sh
```

This will:
- Build Rust core (release mode, no transcription)
- Build Swift app (release configuration)
- Install to `/Applications/Ultra Meeting.app`
- Code sign with ad-hoc signature
- Launch automatically

### Development Workflow

After making code changes:

```bash
./update.sh
```

This quickly rebuilds and updates the installed app. **No permission prompts!**

## Testing

```bash
./test-install.sh
```

Verifies:
- App is installed correctly
- Bundle ID is correct
- App is running
- Rust library is bundled
- Code signing is valid

## Permission Management

### First Run
1. Click the menu bar icon (waveform)
2. Toggle "I understand" for recording disclosure
3. Click "Grant Microphone" → Allow in System Settings
4. Click "Grant Screen Capture" → Allow in System Settings
5. Restart the app (permissions take effect)

### After Updates
Permissions persist automatically. No need to grant them again.

### Checking Permissions

System Settings → Privacy & Security → Microphone/Screen Recording
- You should see only ONE entry: "Ultra Meeting"
- Bundle ID: com.ultrameeting.app

### Resetting Permissions (if needed)

```bash
tccutil reset Microphone com.ultrameeting.app
tccutil reset ScreenCapture com.ultrameeting.app
```

## File Structure

```
/Applications/Ultra Meeting.app/
├── Contents/
│   ├── Info.plist              # Bundle info with ID
│   ├── MacOS/
│   │   └── UltraMeeting        # Main executable
│   ├── Frameworks/
│   │   └── libultra_meeting_core.dylib  # Rust core
│   └── Resources/              # App resources
```

## Scripts

- `install.sh` - Full build and install (first time)
- `update.sh` - Quick rebuild and update (development)
- `test-install.sh` - Verify installation
- `run.sh` - Old script (deprecated, use install.sh)

## Troubleshooting

### App won't launch
```bash
# Check if it's running
pgrep -f UltraMeeting

# Launch manually
open "/Applications/Ultra Meeting.app"

# Or directly
"/Applications/Ultra Meeting.app/Contents/MacOS/UltraMeeting"
```

### Permissions not working
1. Check System Settings → Privacy & Security
2. Ensure "Ultra Meeting" is checked
3. Restart the app
4. If still not working, reset permissions and try again

### Multiple entries in System Settings
This shouldn't happen anymore! The bundle ID ensures only one entry.

If you see old entries:
1. Remove them from System Settings
2. Reinstall: `./install.sh`

### Build errors
```bash
# Clean build
cd rust-core && cargo clean
cd ../UltraMeeting && xcodebuild clean
./install.sh
```

## Next Steps

1. ✅ App is installed and running
2. Grant permissions (first time only)
3. Test recording functionality
4. Make code changes
5. Run `./update.sh` to rebuild
6. Test again (no new permissions needed!)

## Notes

- Permissions are tied to `com.ultrameeting.app` bundle ID
- App uses ad-hoc code signing (fine for local development)
- For distribution, you'd need a proper Developer ID certificate
- The app is a menu bar app (LSUIElement=true), no dock icon
