#!/bin/bash
# Build project in Docker container
# This script isolates Docker operations from the main PM binary

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Check if container is running
if ! docker-compose ps pm-dev | grep -q "Up"; then
    echo "🐳 Starting development container..."
    docker-compose up -d pm-dev
    
    # Wait for container to be ready
    sleep 2
fi

echo "🐳 Building project in Docker container..."

# Build in Docker
docker-compose exec pm-dev cargo build --release

echo "✅ Docker build completed"