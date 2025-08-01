#!/bin/bash

# Test coverage script for PM development
# This script generates code coverage reports using cargo-tarpaulin

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "🧪 Running PM test coverage analysis..."

# Check if cargo-tarpaulin is installed
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo "📦 Installing cargo-tarpaulin..."
    cargo install cargo-tarpaulin
fi

# Create coverage directory
mkdir -p coverage

echo "📊 Generating coverage report..."

# Run tarpaulin with comprehensive settings
cargo tarpaulin \
    --verbose \
    --all-features \
    --workspace \
    --timeout 180 \
    --exclude-files vendor/* \
    --exclude-files target/* \
    --exclude-files benches/* \
    --exclude-files examples/* \
    --exclude-files script/* \
    --out Html \
    --out Xml \
    --output-dir coverage/

echo ""
echo "✅ Coverage analysis complete!"
echo ""
echo "📋 Coverage Results:"
echo "  - HTML Report: coverage/tarpaulin-report.html"
echo "  - XML Report:  coverage/cobertura.xml"
echo ""

# Display basic coverage stats if available
if [ -f coverage/cobertura.xml ]; then
    echo "📊 Coverage Summary:"
    # Extract line coverage from XML (basic parsing)
    if command -v xmllint &> /dev/null; then
        line_rate=$(xmllint --xpath "string(//coverage/@line-rate)" coverage/cobertura.xml 2>/dev/null || echo "")
        if [ -n "$line_rate" ]; then
            percentage=$(echo "$line_rate * 100" | bc -l 2>/dev/null | xargs printf "%.1f" 2>/dev/null || echo "N/A")
            echo "  - Line Coverage: ${percentage}%"
        fi
    fi
fi

echo ""
echo "🌐 To view detailed coverage report:"
echo "   open coverage/tarpaulin-report.html"
echo ""
echo "🚀 Ready for development with coverage insights!"