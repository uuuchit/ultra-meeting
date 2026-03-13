#!/bin/bash
# Quick rebuild and update script for development

set -e

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "🔨 Rebuilding..."

# Build Rust core
cd "$PROJECT_DIR/rust-core"
cargo build --release --no-default-features

# Build Swift app
cd "$PROJECT_DIR/UltraMeeting"
xcodebuild -scheme UltraMeeting -configuration Release build > /dev/null 2>&1

# Find the built app
BUILT_APP=$(find ~/Library/Developer/Xcode/DerivedData/UltraMeeting-*/Build/Products/Release -name "UltraMeeting.app" -type d 2>/dev/null | head -1)

if [ -z "$BUILT_APP" ]; then
    echo "❌ Build failed"
    exit 1
fi

# Kill running instance
killall "UltraMeeting" 2>/dev/null || true
sleep 1

# Update installation
rm -rf "/Applications/Ultra Meeting.app"
cp -R "$BUILT_APP" "/Applications/Ultra Meeting.app"
codesign --force --deep --sign - "/Applications/Ultra Meeting.app" > /dev/null 2>&1

echo "✅ Updated! Launching..."
"/Applications/Ultra Meeting.app/Contents/MacOS/UltraMeeting" > /dev/null 2>&1 &

sleep 2
pgrep -f "UltraMeeting" > /dev/null && echo "✅ Running! Permissions intact." || echo "⚠️  Launch manually"
