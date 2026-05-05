#!/bin/bash

echo "Uninstalling Claude Code Router..."

# Stop running services
ccr stop 2>/dev/null || true

# Remove binaries
rm -f /usr/local/bin/ccr-server
rm -f /usr/local/bin/ccr-cli
rm -f /usr/local/bin/ccr-ui
rm -f /usr/local/bin/ccr

# Ask user if they want to remove config
read -p "Remove configuration files? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf ~/.claude-code-router
    echo "Configuration removed"
fi

echo "✅ Claude Code Router uninstalled"
