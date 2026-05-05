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
