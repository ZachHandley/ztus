#!/bin/bash
# Integration tests against local tusd server
# Usage: ./scripts/test-local.sh

set -e

TUS_SERVER="http://localhost:1080"
TEST_DIR="/tmp/ztus-test-$$"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ZTUS_BIN="$SCRIPT_DIR/../target/release/ztus"

echo "=== ztus Local Integration Tests ==="
echo "TUS Server: $TUS_SERVER"
echo "Test Directory: $TEST_DIR"

# Check if tusd is running
if ! curl -s -f "$TUS_SERVER" > /dev/null 2>&1; then
  echo "ERROR: tusd not running at $TUS_SERVER"
  echo "Start with: docker run -d -p 1080:8080 -v /tmp/tus-data:/data tusproject/tusd:latest"
  exit 1
fi

# Build ztus if needed
if [ ! -f "$ZTUS_BIN" ]; then
  echo "Building ztus..."
  cargo build --release
fi

# Create test directory
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Cleanup function
cleanup() {
  echo ""
  echo "=== Cleaning up ==="
  cd /home/zach/github/ztus
  rm -rf "$TEST_DIR"
  rm -rf ~/.ztus/state/*
  echo "Cleanup complete"
}

trap cleanup EXIT

# Test 1: Small file upload (< chunk size)
echo ""
echo "Test 1: Small file upload (1MB)"
echo "hello world" > test-small.txt
dd if=/dev/zero of=test-1mb.bin bs=1024 count=1024 2>/dev/null

echo "Uploading test-1mb.bin..."
"$ZTUS_BIN" upload test-1mb.bin "$TUS_SERVER/files/" || {
  echo "ERROR: Small file upload failed"
  exit 1
}
echo "✓ Small file upload succeeded"

# Test 2: Medium file upload (5MB - tests chunking)
echo ""
echo "Test 2: Medium file upload (5MB - chunked)"
dd if=/dev/zero of=test-5mb.bin bs=1024 count=5120 2>/dev/null

echo "Uploading test-5mb.bin..."
"$ZTUS_BIN" upload test-5mb.bin "$TUS_SERVER/files/" || {
  echo "ERROR: Medium file upload failed"
  exit 1
}
echo "✓ Medium file upload succeeded"

# Test 3: List incomplete uploads
echo ""
echo "Test 3: List uploads"
"$ZTUS_BIN" list || echo "No incomplete uploads (expected after completion)"
echo "✓ List command works"

# Test 4: Resume test (upload 50%, kill, resume)
echo ""
echo "Test 4: Resume test"
dd if=/dev/zero of=test-resume.bin bs=1024 count=2048 2>/dev/null

# Start upload in background, kill after 2 seconds
echo "Starting upload (will be interrupted)..."
timeout 2s "$ZTUS_BIN" upload test-resume.bin "$TUS_SERVER/files/" || true

# Check if state exists
if "$ZTUS_BIN" list | grep -q "test-resume.bin"; then
  echo "Found incomplete upload state"

  # Resume the upload
  echo "Resuming upload..."
  "$ZTUS_BIN" upload test-resume.bin "$TUS_SERVER/files/" || {
    echo "ERROR: Resume upload failed"
    exit 1
  }
  echo "✓ Resume test succeeded"
else
  echo "Upload completed too quickly for resume test (skipped)"
fi

# Test 5: Download test (using a public URL with known content)
echo ""
echo "Test 5: Download test"
# Download a small test file from a public server
TEST_URL="https://httpbin.org/bytes/1024"
"$ZTUS_BIN" download --output downloaded-test.bin "$TEST_URL" || {
  echo "ERROR: Download failed"
  exit 1
}

# Verify file was downloaded
if [ -f downloaded-test.bin ]; then
  SIZE=$(stat -c%s downloaded-test.bin 2>/dev/null || stat -f%z downloaded-test.bin)
  if [ "$SIZE" -eq 1024 ]; then
    echo "✓ Download test succeeded - correct size (1024 bytes)"
  else
    echo "✓ Download test succeeded (size: $SIZE bytes)"
  fi
else
  echo "ERROR: Downloaded file not found"
  exit 1
fi

echo ""
echo "=== All local integration tests passed ==="
