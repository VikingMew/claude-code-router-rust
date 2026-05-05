#!/bin/bash
set -e

VERSION="0.1.0"
ARCH=$(uname -m)
PKG_NAME="claude-code-router-${VERSION}-${ARCH}.pkg"

echo "Building macOS package: $PKG_NAME"

# Build release binaries
echo "Building release binaries..."
cargo build --release --workspace

# Create package structure
BUILD_DIR="build/macos"
PAYLOAD_DIR="${BUILD_DIR}/payload"
SCRIPTS_DIR="${BUILD_DIR}/scripts"

mkdir -p "${PAYLOAD_DIR}/usr/local/bin"
mkdir -p "${SCRIPTS_DIR}"

# Copy binaries
cp target/release/ccr-server "${PAYLOAD_DIR}/usr/local/bin/"
cp target/release/ccr-cli "${PAYLOAD_DIR}/usr/local/bin/"
cp target/release/ccr-ui "${PAYLOAD_DIR}/usr/local/bin/"

# Copy scripts
cp packaging/macos/scripts/preinstall "${SCRIPTS_DIR}/"
cp packaging/macos/scripts/postinstall "${SCRIPTS_DIR}/"
chmod +x "${SCRIPTS_DIR}"/*

# Build component package
pkgbuild --root "${PAYLOAD_DIR}" \
         --scripts "${SCRIPTS_DIR}" \
         --identifier com.musistudio.ccr \
         --version "${VERSION}" \
         --install-location / \
         "${BUILD_DIR}/ccr-component.pkg"

# Build product archive
mkdir -p dist
productbuild --distribution packaging/macos/Distribution.xml \
             --package-path "${BUILD_DIR}" \
             --resources packaging/macos/resources \
             "dist/${PKG_NAME}"

echo "✅ Package created: dist/${PKG_NAME}"

# Optional: Sign package
if [ -n "$SIGNING_IDENTITY" ]; then
    echo "Signing package..."
    productsign --sign "$SIGNING_IDENTITY" \
                "dist/${PKG_NAME}" \
                "dist/${PKG_NAME%.pkg}-signed.pkg"
    mv "dist/${PKG_NAME%.pkg}-signed.pkg" "dist/${PKG_NAME}"
    echo "✅ Package signed"
fi
