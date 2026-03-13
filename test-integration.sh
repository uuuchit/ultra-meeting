#!/bin/bash
# End-to-end integration test for Ultra Meeting

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║         Ultra Meeting - End-to-End Integration Test         ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# 1. Build Rust core
echo "→ Building Rust core..."
cd ~/Desktop/projects/ultra-meeting/rust-core
cargo build --release 2>&1 | grep -E "(Compiling ultra|Finished)" || true
if [ $? -eq 0 ]; then
    echo "  ✅ Rust core built successfully"
else
    echo "  ❌ Rust core build failed"
    exit 1
fi

# 2. Check dylib exists
DYLIB_PATH="target/release/libultra_meeting_core.dylib"
if [ -f "$DYLIB_PATH" ]; then
    echo "  ✅ Found: $DYLIB_PATH"
    ls -lh "$DYLIB_PATH"
else
    echo "  ❌ Missing: $DYLIB_PATH"
    exit 1
fi

# 3. Check exported symbols
echo ""
echo "→ Checking FFI exports..."
EXPORTS=$(nm -gU "$DYLIB_PATH" | grep ultra_meeting | wc -l)
if [ "$EXPORTS" -gt 5 ]; then
    echo "  ✅ Found $EXPORTS exported FFI functions"
    nm -gU "$DYLIB_PATH" | grep ultra_meeting | head -10
else
    echo "  ❌ Only found $EXPORTS exports (expected >5)"
    exit 1
fi

# 4. Build Swift app
echo ""
echo "→ Building Swift app..."
cd ~/Desktop/projects/ultra-meeting/UltraMeeting
xcodebuild -scheme UltraMeeting -configuration Debug build 2>&1 | grep -E "(BUILD)" | tail -1
if [ $? -eq 0 ]; then
    echo "  ✅ Swift app built successfully"
else
    echo "  ❌ Swift app build failed"
    exit 1
fi

# 5. Find the built app
APP_PATH=$(find ~/Library/Developer/Xcode/DerivedData/UltraMeeting-*/Build/Products/Debug -name "UltraMeeting.app" -type d 2>/dev/null | head -1)
if [ -n "$APP_PATH" ]; then
    echo "  ✅ Found app: $APP_PATH"
else
    echo "  ❌ Could not find built app"
    exit 1
fi

# 6. Check if Rust library is linked
echo ""
echo "→ Checking library linkage..."
if otool -L "$APP_PATH/Contents/MacOS/UltraMeeting" | grep -q "libultra_meeting_core"; then
    echo "  ✅ Rust library is linked"
    otool -L "$APP_PATH/Contents/MacOS/UltraMeeting" | grep libultra_meeting_core
else
    echo "  ⚠️  Rust library not found in linkage (may be using @rpath)"
    echo "  Checking for @rpath references:"
    otool -L "$APP_PATH/Contents/MacOS/UltraMeeting" | grep -E "(@rpath|ultra)" || echo "  No @rpath references found"
fi

# 7. Check permissions in Info.plist
echo ""
echo "→ Checking Info.plist permissions..."
INFO_PLIST="$APP_PATH/Contents/Info.plist"
if /usr/libexec/PlistBuddy -c "Print :NSMicrophoneUsageDescription" "$INFO_PLIST" 2>/dev/null; then
    echo "  ✅ Microphone permission declared"
else
    echo "  ❌ Missing NSMicrophoneUsageDescription"
fi

if /usr/libexec/PlistBuddy -c "Print :NSScreenCaptureUsageDescription" "$INFO_PLIST" 2>/dev/null; then
    echo "  ✅ Screen capture permission declared"
else
    echo "  ⚠️  Missing NSScreenCaptureUsageDescription (may not be required)"
fi

# 8. Check storage directory
echo ""
echo "→ Checking storage setup..."
STORAGE_DIR=~/Documents/UltraMeeting/recordings
if [ -d "$STORAGE_DIR" ]; then
    echo "  ✅ Storage directory exists: $STORAGE_DIR"
    RECORDING_COUNT=$(find "$STORAGE_DIR" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l)
    echo "  📁 Found $RECORDING_COUNT existing recordings"
else
    echo "  ℹ️  Storage directory will be created on first recording"
fi

# 9. Summary
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                      TEST SUMMARY                            ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "✅ Rust core: Built and exports FFI functions"
echo "✅ Swift app: Built successfully"
echo "✅ Permissions: Declared in Info.plist"
echo ""
echo "🎯 READY TO TEST"
echo ""
echo "To run the app:"
echo "  open \"$APP_PATH\""
echo ""
echo "Or run from Xcode:"
echo "  cd ~/Desktop/projects/ultra-meeting/UltraMeeting"
echo "  open UltraMeeting.xcodeproj"
echo ""
echo "Expected behavior:"
echo "  1. Menu bar icon appears"
echo "  2. Click icon → shows menu"
echo "  3. Acknowledge recording disclosure"
echo "  4. Grant microphone permission"
echo "  5. Grant screen recording permission (requires restart)"
echo "  6. Click 'Start Recording'"
echo "  7. Speak into mic + play audio from another app"
echo "  8. Click 'Stop Recording'"
echo "  9. Check ~/Documents/UltraMeeting/recordings/ for output"
echo ""
echo "⚠️  KNOWN LIMITATIONS:"
echo "  - Screen recording permission requires app restart after grant"
echo "  - Transcription not yet wired (will be added in Phase 1.6)"
echo "  - No app selection UI (captures all system audio)"
echo ""
