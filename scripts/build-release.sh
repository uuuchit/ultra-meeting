#!/bin/bash
# Build release app bundle and prepare for signing/notarization.
# See docs/release-checklist.md for full process.

set -e
cd "$(dirname "$0")/.."

echo "Building Rust core..."
cd rust-core
cargo build --release --no-default-features
# For transcription: cargo build --release --features transcription
cd ..

echo "Building UltraMeeting.app..."
xcodebuild -scheme UltraMeeting -configuration Release -project UltraMeeting/UltraMeeting.xcodeproj clean build 2>&1 | tail -20

echo ""
echo "App built. Find it in ~/Library/Developer/Xcode/DerivedData or check xcodebuild output."
echo ""
echo "Next: codesign and notarize (see docs/release-checklist.md)"
