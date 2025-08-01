#!/bin/bash

# End-to-End test script for 'pm' based on TESTING_GUIDE.md
# This script is intended to be run inside the Docker container.

set -e # Exit immediately if a command exits with a non-zero status.
set -x # Print commands and their arguments as they are executed.

# Use the pm binary from the system PATH (installed in Dockerfile.manual)
PM_BIN="pm"

# Create a clean test directory
TEST_DIR="/tmp/pm-e2e-test"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "====== SCENARIO 1: Project Initialization & Status ======"
$PM_BIN init
$PM_BIN status | grep "No projects found"

echo "====== SCENARIO 2: Tag & Project Management ======"
# Create a dummy project directory
DUMMY_PROJECT_DIR="$TEST_DIR/service-a"
mkdir -p "$DUMMY_PROJECT_DIR"

$PM_BIN tag add backend frontend
$PM_BIN tag list | grep "backend"
$PM_BIN tag list | grep "frontend"

$PM_BIN project add "$DUMMY_PROJECT_DIR" --tags backend
$PM_BIN project list --tags backend | grep "service-a"

echo "====== SCENARIO 3: Network Extension Add (Git Clone) ======"
# Using a known, simple, and lightweight git repository for testing
# Using https instead of ssh to avoid key issues in the container
$PM_BIN extension add https://github.com/rust-lang/git2-rs.git
$PM_BIN extension list | grep "git2-rs"

echo "====== SCENARIO 4: Configuration Management ======"
$PM_BIN config set backup.path /new/backup/path
$PM_BIN config get backup.path | grep "/new/backup/path"

echo "====== SCENARIO 5: Backup & Restore ======"
# Check for the config file before backup
if [ ! -f ".pm/config.toml" ]; then
    echo "Config file .pm/config.toml does not exist before backup!"
    exit 1
fi

$PM_BIN backup create --reason "E2E Test Backup"
# Simulate disaster
rm .pm/config.toml
if [ -f ".pm/config.toml" ]; then
    echo "Failed to delete .pm/config.toml for restore test!"
    exit 1
fi

# Restore
LATEST_BACKUP=$($PM_BIN backup list | grep "E2E Test Backup" | head -n 1 | cut -d' ' -f1)
$PM_BIN backup restore --id "$LATEST_BACKUP"

# Check if the file is restored
if [ ! -f ".pm/config.toml" ]; then
    echo "Config file .pm/config.toml was not restored!"
    exit 1
fi

echo "====== All E2E scenarios passed successfully! ======"
