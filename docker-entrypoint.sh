#!/bin/bash
# Docker entrypoint script for PM manual testing container
# Fixes volume permission issues and ensures pmuser can write to config directory

set -e

# Fix permissions for mounted config volume
if [ -d "/home/pmuser/.config/pm" ]; then
    # Only change ownership if it's currently owned by root
    if [ "$(stat -c '%U' /home/pmuser/.config/pm)" = "root" ]; then
        echo "🔧 Fixing volume permissions for PM config directory..."
        chown -R pmuser:pmuser /home/pmuser/.config/pm
        echo "✅ Volume permissions fixed"
    fi
fi

# If running as root, switch to pmuser for the actual command
if [ "$(id -u)" = "0" ]; then
    # Execute the command as pmuser
    exec gosu pmuser "$@"
else
    # Already running as pmuser, execute directly
    exec "$@"
fi