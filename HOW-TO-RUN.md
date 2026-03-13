# How to Run Ultra Meeting on Your Mac

## Quick Start (Recommended)

### Option 1: Run from Xcode

1. **Open the project:**
   ```bash
   cd ~/Desktop/projects/ultra-meeting/UltraMeeting
   open UltraMeeting.xcodeproj
   ```

2. **In Xcode:**
   - Click the ▶️ Play button (top left)
   - Or press `Cmd + R`

3. **Look for the menu bar icon:**
   - Check the top-right corner of your screen
   - You should see a waveform icon (○)

---

## First Time Setup

### Step 1: Grant Permissions

1. Click the menu bar icon
2. Toggle "I understand" for recording disclosure
3. Click "Grant Microphone" → Allow
4. Click "Grant Screen Capture" → Allow
5. **⚠️ IMPORTANT:** Quit and restart the app after granting screen capture

### Step 2: Test Recording

1. Click "Start Recording"
2. Speak into your microphone
3. Play audio from another app (YouTube, Spotify, etc.)
4. Wait 30 seconds
5. Click "Stop Recording"

### Step 3: Check Output

```bash
open ~/Documents/UltraMeeting/recordings/
```

You should see a folder with:
- `metadata.yaml` - recording info
- `mic_000.wav`, `mic_001.wav` - your voice
- `remote_000.wav`, `remote_001.wav` - system audio

---

## Troubleshooting

### App doesn't appear in menu bar

**Solution:** Run from Xcode instead of Finder
```bash
cd ~/Desktop/projects/ultra-meeting/UltraMeeting
open UltraMeeting.xcodeproj
# Then click ▶️ in Xcode
```

### "Library not loaded" error

**Solution:** Rebuild from Xcode
1. Open UltraMeeting.xcodeproj
2. Product → Clean Build Folder (Cmd + Shift + K)
3. Product → Build (Cmd + B)
4. Product → Run (Cmd + R)

### Screen recording permission doesn't work

**Solution:** Restart the app
1. Click menu bar icon → Quit
2. Run again from Xcode
3. Permission should now be active

### No audio in recordings

**Possible causes:**
1. Microphone permission not granted
2. Screen recording permission not granted
3. App not restarted after granting screen recording
4. No audio playing during recording

**Solution:**
1. Check System Settings → Privacy & Security → Microphone
2. Check System Settings → Privacy & Security → Screen Recording
3. Make sure UltraMeeting is checked in both
4. Restart the app

---

## Alternative: Build and Run from Terminal

If Xcode doesn't work, try:

```bash
# Build Rust core
cd ~/Desktop/projects/ultra-meeting/rust-core
cargo build --release

# Build Swift app
cd ~/Desktop/projects/ultra-meeting/UltraMeeting
xcodebuild -scheme UltraMeeting -configuration Debug build

# Run from Xcode (easier than terminal)
open UltraMeeting.xcodeproj
# Then click ▶️
```

---

## What to Expect

### Menu Bar States

- **Idle**: Waveform icon (○)
- **Recording**: Red circle icon (●)
- **Processing**: Waveform with animation
- **Error**: Warning triangle (⚠️)

### Recording Flow

```
Click Start
  ↓
State: "Recording"
  ↓
Speak + play audio
  ↓
Click Stop
  ↓
State: "Stopping" → "Processing" → "Idle"
  ↓
Check ~/Documents/UltraMeeting/recordings/
```

### Expected Output

For a 5-minute recording:
- Folder: `2026-03-08_18-30-00_meeting-name/`
- Files:
  - `metadata.yaml` (~1 KB)
  - `mic_000.wav` through `mic_060.wav` (~30 MB total)
  - `remote_000.wav` through `remote_060.wav` (~30 MB total)
- Total size: ~60 MB for 5 minutes

---

## Known Issues

1. **Screen recording permission requires restart** - This is a macOS limitation
2. **No transcription yet** - Feature exists but not enabled (needs `--features transcription`)
3. **No app selection** - Records all system audio, not specific apps
4. **No visual feedback** - Duration not shown during recording

---

## Quick Commands

```bash
# Open project in Xcode
cd ~/Desktop/projects/ultra-meeting/UltraMeeting && open UltraMeeting.xcodeproj

# View recordings
open ~/Documents/UltraMeeting/recordings/

# Check if app is running
pgrep -f UltraMeeting

# View logs (if app crashes)
log show --predicate 'process == "UltraMeeting"' --last 5m
```

---

## Next Steps After Testing

If recording works:
1. Enable transcription (see docs/test-report.md)
2. Test with real meeting (Zoom, Google Meet)
3. Verify audio quality and sync

If recording fails:
1. Check Console.app for errors
2. Verify permissions in System Settings
3. Try rebuilding from Xcode
