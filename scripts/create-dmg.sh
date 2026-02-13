#!/usr/bin/env bash
# Simple DMG creator that doesn't require Finder/AppleScript.
# Usage: ./scripts/create-dmg.sh
set -euo pipefail

APP_NAME="NoString"
VERSION="0.1.0"
ARCH="$(uname -m)"
APP_PATH="target/release/bundle/macos/${APP_NAME}.app"
DMG_NAME="${APP_NAME}_${VERSION}_${ARCH}.dmg"
DMG_DIR="target/release/bundle/dmg"
STAGING="/tmp/dmg-staging-$$"

if [ ! -d "$APP_PATH" ]; then
  echo "Error: $APP_PATH not found. Run 'cargo tauri build' first."
  exit 1
fi

echo "Creating DMG..."
mkdir -p "$DMG_DIR" "$STAGING"
cp -R "$APP_PATH" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

# Create DMG
hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$STAGING" \
  -ov \
  -format UDZO \
  "${DMG_DIR}/${DMG_NAME}"

rm -rf "$STAGING"
echo "✅ DMG created: ${DMG_DIR}/${DMG_NAME}"
ls -lh "${DMG_DIR}/${DMG_NAME}"
