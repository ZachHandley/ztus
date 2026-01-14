#!/bin/bash
# Run all ztus tests
# Usage: ./scripts/test-all.sh [API_KEY]

set -e

echo "========================================"
echo "=== ztus Test Suite               ==="
echo "========================================"

# Get API key from argument or env
if [ -n "$1" ]; then
  API_KEY="$1"
fi

# Run unit tests
echo ""
echo "=== Phase 1: Unit Tests ==="
echo ""
cargo test --quiet
echo "✓ Unit tests passed"

# Run local integration tests
echo ""
echo "=== Phase 2: Local Integration Tests (tusd) ==="
echo ""
if ./scripts/test-local.sh; then
  echo "✓ Local integration tests passed"
else
  echo "✗ Local integration tests failed"
  echo ""
  echo "Note: Make sure tusd is running on port 1080:"
  echo "  docker run -d -p 1080:8080 -v /tmp/tus-data:/data tusproject/tusd:latest"
  exit 1
fi

# Run remote tests if API key is available
echo ""
echo "=== Phase 3: Remote Integration Tests (files.zachszone.com) ==="
echo ""

if [ -n "$API_KEY" ]; then
  if API_KEY="$API_KEY" ./scripts/test-remote.sh; then
    echo "✓ Remote integration tests passed"
  else
    echo "✗ Remote integration tests failed"
    exit 1
  fi
else
  echo "Skipping remote tests (no API_KEY provided)"
  echo "To run remote tests:"
  echo "  API_KEY=xxx ./scripts/test-all.sh"
  echo "  or"
  echo "  ./scripts/test-all.sh xxx"
fi

echo ""
echo "========================================"
echo "=== All Tests Passed!               ==="
echo "========================================"
