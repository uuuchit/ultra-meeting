#!/bin/bash
# Build and install UltraMeeting as a standalone local macOS app.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT_DIR="${ROOT_DIR}/UltraMeeting"
DERIVED_DATA="${PROJECT_DIR}/build"
APP_NAME="UltraMeeting.app"
BUILT_APP="${DERIVED_DATA}/Build/Products/Release/${APP_NAME}"
INSTALL_APP="/Applications/${APP_NAME}"

echo "Building UltraMeeting Release app..."
xcodebuild \
  -project "${PROJECT_DIR}/UltraMeeting.xcodeproj" \
  -scheme UltraMeeting \
  -configuration Release \
  -derivedDataPath "${DERIVED_DATA}" \
  build

if [[ ! -d "${BUILT_APP}" ]]; then
  echo "Built app not found: ${BUILT_APP}" >&2
  exit 1
fi

echo "Stopping any running UltraMeeting instance..."
osascript -e 'tell application "UltraMeeting" to quit' >/dev/null 2>&1 || true
sleep 1

echo "Installing to ${INSTALL_APP}..."
rm -rf "${INSTALL_APP}"
ditto "${BUILT_APP}" "${INSTALL_APP}"

echo "Removing quarantine attributes..."
xattr -dr com.apple.quarantine "${INSTALL_APP}" >/dev/null 2>&1 || true

echo "Verifying code signature..."
codesign --verify --deep --strict "${INSTALL_APP}"

echo "Registering with LaunchServices..."
/System/Library/Frameworks/CoreServices.framework/Versions/Current/Frameworks/LaunchServices.framework/Versions/Current/Support/lsregister \
  -f -R -trusted "${INSTALL_APP}"

echo "Launching ${INSTALL_APP}..."
open "${INSTALL_APP}"

echo "Installed: ${INSTALL_APP}"
