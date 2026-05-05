#!/bin/bash
set -euo pipefail

VERSION="${VERSION:-0.1.0}"
APP_NAME="Claude Code Router.app"
BUILD_DIR="build/macos-app"
APP_DIR="${BUILD_DIR}/${APP_NAME}"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
BIN_DIR="${RESOURCES_DIR}/bin"

echo "Building macOS .app bundle..."
cargo build --release --workspace

rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}" "${BIN_DIR}"

cp target/release/ccr-ui "${MACOS_DIR}/ccr-ui"
cp target/release/ccr-server "${BIN_DIR}/ccr-server"
cp target/release/ccr "${BIN_DIR}/ccr"

cp packaging/macos-dmg/Info.plist "${CONTENTS_DIR}/Info.plist"
printf 'APPL????' > "${CONTENTS_DIR}/PkgInfo"

cat > "${RESOURCES_DIR}/install-cli.sh" <<'EOF'
#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="${SCRIPT_DIR}/bin"

if [ ! -d /usr/local/bin ]; then
  sudo mkdir -p /usr/local/bin
fi

sudo ln -sf "${BIN_DIR}/ccr" /usr/local/bin/ccr
sudo ln -sf "${BIN_DIR}/ccr-server" /usr/local/bin/ccr-server

echo "CCR command line tools installed to /usr/local/bin"
EOF
chmod +x "${RESOURCES_DIR}/install-cli.sh"

if [ -f "packaging/macos-dmg/icon.icns" ]; then
  cp packaging/macos-dmg/icon.icns "${RESOURCES_DIR}/icon.icns"
elif command -v python3 >/dev/null 2>&1; then
  python3 packaging/macos-dmg/create-icns.py "${RESOURCES_DIR}/icon.icns"
  rm -rf "${RESOURCES_DIR}/.icon-pngs"
else
  echo "Warning: python3 not available; app bundle will not include icon.icns"
fi

echo "App bundle created: ${APP_DIR}"
