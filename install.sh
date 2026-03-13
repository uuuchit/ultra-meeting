#!/bin/bash
# Install Ultra Meeting to /Applications

set -e

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_NAME="Ultra Meeting.app"
INSTALL_PATH="/Applications/$APP_NAME"

echo "🔨 Building Ultra Meeting..."

# Build Rust core
cd "$PROJECT_DIR/rust-core"
cargo build --release --no-default-features

# Build Swift app
cd "$PROJECT_DIR/UltraMeeting"
xcodebuild -scheme UltraMeeting -configuration Release build

# Find the built app
BUILT_APP=$(find ~/Library/Developer/Xcode/DerivedData/UltraMeeting-*/Build/Products/Release -name "UltraMeeting.app" -type d 2>/dev/null | head -1)

if [ -z "$BUILT_APP" ]; then
    echo "❌ Build failed - app not found"
    exit 1
fi

echo "✅ Build complete"
echo ""

# Kill running instance if exists
if pgrep -f "Ultra Meeting" > /dev/null; then
    echo "🛑 Stopping running instance..."
    killall "Ultra Meeting" 2>/dev/null || true
    sleep 1
fi

# Remove old installation
if [ -d "$INSTALL_PATH" ]; then
    echo "🗑️  Removing old installation..."
    rm -rf "$INSTALL_PATH"
fi

# Install to /Applications
echo "📦 Installing to /Applications..."
cp -R "$BUILT_APP" "$INSTALL_PATH"

# Code sign with ad-hoc signature (for local development)
echo "✍️  Signing app..."
codesign --force --deep --sign - "$INSTALL_PATH"

echo ""
echo "✅ Installation complete!"
echo ""
echo "🚀 Launching Ultra Meeting..."
open -a "Ultra Meeting"

sleep 2

if pgrep -f "Ultra Meeting" > /dev/null; then
    echo "✅ App is running - check your menu bar!"
    echo ""
    echo "📝 Note: Permissions are tied to bundle ID (com.ultrameeting.app)"
    echo "   You only need to grant them once, not after each build."
else
    echo "⚠️  App may not have started - try manually: open -a 'Ultra Meeting'"
fi

