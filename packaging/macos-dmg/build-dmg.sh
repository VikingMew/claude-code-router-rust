#!/bin/bash
set -euo pipefail

VERSION="${VERSION:-0.1.0}"
APP_NAME="Claude Code Router.app"
DMG_NAME="Claude-Code-Router-${VERSION}-$(uname -m).dmg"
APP_BUILD_DIR="build/macos-app"
DMG_ROOT="build/dmg-root"
DIST_DIR="dist"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "DMG creation requires macOS."
  exit 1
fi

if [ ! -d "${APP_BUILD_DIR}/${APP_NAME}" ]; then
  packaging/macos-dmg/build-app.sh
fi

rm -rf "${DMG_ROOT}"
mkdir -p "${DMG_ROOT}" "${DIST_DIR}"

cp -R "${APP_BUILD_DIR}/${APP_NAME}" "${DMG_ROOT}/${APP_NAME}"
ln -s /Applications "${DMG_ROOT}/Applications"

if [ -f "packaging/macos-dmg/README.txt" ]; then
  cp packaging/macos-dmg/README.txt "${DMG_ROOT}/README.txt"
fi

hdiutil create \
  -volname "Claude Code Router" \
  -srcfolder "${DMG_ROOT}" \
  -ov \
  -format UDZO \
  "${DIST_DIR}/${DMG_NAME}"

rm -rf "${DMG_ROOT}"

echo "DMG created: ${DIST_DIR}/${DMG_NAME}"
