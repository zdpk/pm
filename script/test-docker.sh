#!/bin/bash
# Run tests in Docker container
# This script isolates Docker operations from the main PM binary

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "🐳 Running tests in Docker container..."

# Run tests in Docker
docker-compose run --rm pm-test

echo "✅ Docker tests completed"