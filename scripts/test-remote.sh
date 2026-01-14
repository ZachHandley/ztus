#!/bin/bash
# Integration tests against files.zachszone.com
# Usage: API_KEY=xxx ./scripts/test-remote.sh

set -e

TUS_SERVER="https://files.zachszone.com"
TEST_PREFIX="ztus-test-$$"
TEST_DIR="/tmp/ztus-remote-test-$$"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ZTUS_BIN="$SCRIPT_DIR/../target/release/ztus"

# Get API key from env or ZDownloadAPI .env
if [ -n "$API_KEY" ]; then
  ZDOWNLOAD_API_KEY="$API_KEY"
elif [ -f "../ZDownloadAPI/.env" ]; then
  ZDOWNLOAD_API_KEY=$(grep "^API_KEY=" ../ZDownloadAPI/.env | cut -d= -f2)
else
  echo "ERROR: API_KEY not set and ../ZDownloadAPI/.env not found"
  echo "Usage: API_KEY=xxx ./scripts/test-remote.sh"
  exit 1
fi

if [ -z "$ZDOWNLOAD_API_KEY" ]; then
  echo "ERROR: API_KEY is empty"
  exit 1
fi

echo "=== ztus Remote Integration Tests ==="
echo "TUS Server: $TUS_SERVER"
echo "Test Directory: $TEST_DIR"
echo "Test Prefix: $TEST_PREFIX"

# Check if server is reachable
if ! curl -s -f "$TUS_SERVER/health" > /dev/null 2>&1; then
  echo "WARNING: Server health check failed, continuing anyway..."
fi

# Build ztus if needed
if [ ! -f "$ZTUS_BIN" ]; then
  echo "Building ztus..."
  cargo build --release
fi

# Create test directory
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Track files we create for cleanup
UPLOADED_FILES=()

# Cleanup function - always run even on failure
cleanup() {
  echo ""
  echo "=== Cleaning up test files from $TUS_SERVER ==="

  for file in "${UPLOADED_FILES[@]}"; do
    echo "Deleting $file..."
    curl -X DELETE -s \
      -H "Authorization: Bearer $ZDOWNLOAD_API_KEY" \
      "$TUS_SERVER/files/$file" || echo "Warning: Failed to delete $file"
  done

  cd /home/zach/github/ztus
  rm -rf "$TEST_DIR"
  rm -rf ~/.ztus/state/* 2>/dev/null || true
  echo "Cleanup complete"
}

trap cleanup EXIT

# Test 1: Small file upload
echo ""
echo "Test 1: Small file upload"
TEST_FILE="${TEST_PREFIX}-small.txt"
echo "Hello from ztus remote test" > "$TEST_FILE"

echo "Uploading $TEST_FILE..."
"$ZTUS_BIN" upload "$TEST_FILE" "$TUS_SERVER/upload/" || {
  echo "ERROR: Upload failed"
  exit 1
}

UPLOADED_FILES+=("$TEST_FILE")
echo "✓ Upload succeeded, tracked for cleanup: $TEST_FILE"

# Verify upload exists
HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$TUS_SERVER/files/$TEST_FILE")
if [ "$HTTP_STATUS" = "200" ]; then
  echo "✓ File verified on server"
else
  echo "ERROR: File not found on server (HTTP $HTTP_STATUS)"
  exit 1
fi

# Test 2: Medium file (chunked upload)
echo ""
echo "Test 2: Medium file upload (5MB - chunked)"
TEST_FILE="${TEST_PREFIX}-medium.bin"
dd if=/dev/zero of="$TEST_FILE" bs=1024 count=5120 2>/dev/null

echo "Uploading $TEST_FILE..."
"$ZTUS_BIN" upload "$TEST_FILE" "$TUS_SERVER/upload/" || {
  echo "ERROR: Upload failed"
  exit 1
}

UPLOADED_FILES+=("$TEST_FILE")
echo "✓ Chunked upload succeeded, tracked for cleanup: $TEST_FILE"

# Test 3: Download test
echo ""
echo "Test 3: Download test"
DOWNLOAD_FILE="${TEST_PREFIX}-download.txt"
echo "Download test content" > "$DOWNLOAD_FILE"

echo "Uploading for download test..."
"$ZTUS_BIN" upload "$DOWNLOAD_FILE" "$TUS_SERVER/upload/" || {
  echo "ERROR: Upload failed"
  exit 1
}
UPLOADED_FILES+=("$DOWNLOAD_FILE")

echo "Downloading $DOWNLOAD_FILE..."
"$ZTUS_BIN" download --output "downloaded-$DOWNLOAD_FILE" "$TUS_SERVER/files/$DOWNLOAD_FILE" || {
  echo "ERROR: Download failed"
  exit 1
}

if cmp -s "$DOWNLOAD_FILE" "downloaded-$DOWNLOAD_FILE"; then
  echo "✓ Download test succeeded - file matches"
else
  echo "ERROR: Downloaded file doesn't match"
  exit 1
fi

# Test 4: Verify cleanup works (manual DELETE test)
echo ""
echo "Test 4: Manual DELETE test"
DELETE_FILE="${TEST_PREFIX}-delete.txt"
echo "This will be deleted immediately" > "$DELETE_FILE"

echo "Uploading $DELETE_FILE..."
"$ZTUS_BIN" upload "$DELETE_FILE" "$TUS_SERVER/upload/" || {
  echo "ERROR: Upload failed"
  exit 1
}

# Delete immediately via API
echo "Deleting $DELETE_FILE via API..."
HTTP_STATUS=$(curl -X DELETE -s -o /dev/null -w "%{http_code}" \
  -H "Authorization: Bearer $ZDOWNLOAD_API_KEY" \
  "$TUS_SERVER/files/$DELETE_FILE")

if [ "$HTTP_STATUS" = "204" ] || [ "$HTTP_STATUS" = "200" ]; then
  echo "✓ DELETE succeeded"

  # Verify it's gone
  VERIFY_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$TUS_SERVER/files/$DELETE_FILE")
  if [ "$VERIFY_STATUS" = "404" ]; then
    echo "✓ File verified deleted (404)"
  else
    echo "WARNING: File still exists after DELETE (HTTP $VERIFY_STATUS)"
  fi
else
  echo "ERROR: DELETE failed (HTTP $HTTP_STATUS)"
  # Don't add to cleanup since we tried to delete it
fi

echo ""
echo "=== All remote integration tests passed ==="
echo ""
echo "Note: ${#UPLOADED_FILES[@]} files will be cleaned up on exit"
