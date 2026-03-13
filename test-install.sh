#!/bin/bash
# Test script to verify Ultra Meeting installation

echo "🧪 Testing Ultra Meeting Installation"
echo ""

# Check if app exists
if [ ! -d "/Applications/Ultra Meeting.app" ]; then
    echo "❌ App not found in /Applications"
    exit 1
fi
echo "✅ App installed in /Applications"

# Check bundle ID
BUNDLE_ID=$(defaults read "/Applications/Ultra Meeting.app/Contents/Info.plist" CFBundleIdentifier 2>/dev/null)
if [ "$BUNDLE_ID" != "com.ultrameeting.app" ]; then
    echo "❌ Wrong bundle ID: $BUNDLE_ID"
    exit 1
fi
echo "✅ Bundle ID correct: $BUNDLE_ID"

# Check if app is running
if pgrep -f "Ultra Meeting" > /dev/null; then
    echo "✅ App is running"
else
    echo "⚠️  App not running - launching..."
    open -a "Ultra Meeting"
    sleep 3
    if pgrep -f "Ultra Meeting" > /dev/null; then
        echo "✅ App launched successfully"
    else
        echo "❌ App failed to launch"
        exit 1
    fi
fi

# Check if Rust library exists
if [ -f "/Applications/Ultra Meeting.app/Contents/Frameworks/libultra_meeting_core.dylib" ]; then
    echo "✅ Rust core library bundled"
else
    echo "❌ Rust core library missing"
    exit 1
fi

# Check entitlements
ENTITLEMENTS=$(codesign -d --entitlements - "/Applications/Ultra Meeting.app" 2>/dev/null | grep -c "com.apple.security")
if [ "$ENTITLEMENTS" -gt 0 ]; then
    echo "✅ App has security entitlements"
else
    echo "⚠️  No security entitlements found"
fi

echo ""
echo "✅ All tests passed!"
echo ""
echo "📝 Next steps:"
echo "   1. Look for the waveform icon in your menu bar"
echo "   2. Click it and grant permissions"
echo "   3. Test recording functionality"
echo ""
echo "💡 After granting permissions, they will persist across builds"
