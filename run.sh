#!/bin/bash
# Quick start script for Ultra Meeting

set -e

echo "🚀 Starting Ultra Meeting..."
echo ""

# Find the built app
APP_PATH=$(find ~/Library/Developer/Xcode/DerivedData/UltraMeeting-*/Build/Products/Debug -name "UltraMeeting.app" -type d 2>/dev/null | head -1)

if [ -z "$APP_PATH" ]; then
    echo "❌ App not found. Building first..."
    cd ~/Desktop/projects/ultra-meeting/UltraMeeting
    xcodebuild -scheme UltraMeeting -configuration Debug build
    APP_PATH=$(find ~/Library/Developer/Xcode/DerivedData/UltraMeeting-*/Build/Products/Debug -name "UltraMeeting.app" -type d 2>/dev/null | head -1)
fi

if [ -n "$APP_PATH" ]; then
    echo "✅ Launching: $APP_PATH"
    open "$APP_PATH"
    sleep 2
    
    if pgrep -f "UltraMeeting.app" > /dev/null; then
        echo ""
        echo "✅ Ultra Meeting is running!"
        echo ""
        echo "👀 Look for the waveform icon in your menu bar (top right)"
        echo ""
        echo "📝 Next steps:"
        echo "   1. Click the menu bar icon"
        echo "   2. Toggle 'I understand' for recording disclosure"
        echo "   3. Click 'Grant Microphone' and allow"
        echo "   4. Click 'Grant Screen Capture' and allow"
        echo "   5. ⚠️  IMPORTANT: Quit and restart the app after granting screen capture"
        echo "   6. Click 'Start Recording'"
        echo "   7. Speak into your mic and play audio from another app"
        echo "   8. Click 'Stop Recording'"
        echo "   9. Check ~/Documents/UltraMeeting/recordings/ for output"
        echo ""
    else
        echo "❌ App failed to start. Check Console.app for errors."
    fi
else
    echo "❌ Could not find or build app"
    exit 1
fi
