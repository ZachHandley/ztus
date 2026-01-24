# ztus

ztus (like yeetus but with a z) - a blazingly fast Rust CLI tool for resumable uploads using the TUS protocol and chunked downloads.

## About

A TUS (tus.io) client because existing ones were out of date or abandoned. Implements the [TUS resumable upload protocol](https://tus.io/protocols/resumable-upload) for reliable file uploads that can be interrupted and resumed, plus wget/curl-like chunked downloads.

**Performance**: ztus achieves **50-100+ MiB/s** on gigabit networks with adaptive chunk sizing and HTTP/2 support - a **40-50x improvement** over traditional TUS clients. Upload a 24 GB file in ~5-10 minutes instead of ~4 hours.

## Features

- **Adaptive Chunk Sizing**: Automatically optimizes chunk sizes based on network conditions (1 MB - 200 MB)
- **HTTP/2 Support**: Leverages HTTP/2 multiplexing when available for 20-30% additional performance
- **Resumable Uploads**: Interrupt and resume uploads at any point with robust crash recovery
- **High Performance**: 50-100+ MiB/s throughput on gigabit networks
- **Progress Reporting**: Real-time progress bars with speed, ETA, and adaptive behavior indicators
- **Checksum Verification**: SHA1/SHA256 support for data integrity (server-side validation)
- **HTTP Range Downloads**: wget/curl-like downloads with resume support
- **State Persistence**: Automatic resume state storage in `~/.ztus/` with enhanced crash recovery
- **TUS Protocol Extensions**: Support for creation, termination, checksum, expiration, and concatenation
- **Cross-Platform**: Native binaries for Linux, macOS (amd64/arm64), and Windows
- **Python Bindings**: PyO3-based Python API with both sync and async support

## Install

```bash
# Build and install locally from source
cargo install --path .
```

## Build

```bash
cargo build --release
```

The release binary is at `target/release/ztus`.

## Test

```bash
cargo test
```

## Usage

### Upload

```bash
# Basic upload (adaptive chunk sizing enabled by default)
ztus upload myfile.zip https://files.example.com/upload/

# Upload with verbose output (shows adaptive behavior)
ztus upload myfile.zip https://files.example.com/upload/ -v

# Disable adaptive sizing and use fixed chunk size
ztus upload myfile.zip https://files.example.com/upload/ --no-adaptive

# Use specific chunk size (disables adaptive sizing)
ztus upload myfile.zip https://files.example.com/upload/ -c 5242880

# Resume interrupted upload
ztus resume myfile.zip https://files.example.com/upload/
```

**Adaptive Chunk Sizing** is enabled by default and automatically optimizes performance:
- Starts at 10 MB and adapts based on network conditions
- Ranges from 1 MB (min) to 200 MB (max)
- Doubles chunk size when throughput is stable and increasing
- Halves chunk size when throughput degrades
- Achieves 40-50x faster uploads compared to fixed 5 MB chunks

To customize adaptive behavior, create `~/.ztus/config.toml`:

```toml
[upload.adaptive]
enabled = true
initial_chunk_size = 10485760  # 10 MB
min_chunk_size = 5242880      # 5 MB
max_chunk_size = 104857600    # 100 MB
adaptation_interval = 5       # Re-evaluate every 5 chunks
stability_threshold = 0.1     # 10% variance threshold
```

### Download

```bash
# Basic download
ztus download https://files.example.com/myfile.zip -O myfile.zip

# Download with custom chunk size (default: 5MB)
ztus download https://files.example.com/myfile.zip -O myfile.zip -c 10485760

# Resume interrupted download (automatic)
ztus download https://files.example.com/myfile.zip -O myfile.zip
```

The download command supports:
- **HTTP Range requests** for efficient resumable downloads
- **Automatic resume** - re-running the same command will continue from where it left off
- **Progress tracking** with real-time progress bar and ETA
- **State persistence** - download progress saved to `~/.ztus/downloads/`
- **Error recovery** - automatic retry with exponential backoff
- **Server compatibility** - works with any HTTP/1.1 server supporting Range requests

### Management

```bash
# List incomplete uploads
ztus list

# Clean up old incomplete uploads (older than 7 days)
ztus cleanup --days 7

# Show configuration
ztus info
```

## Python Usage

ztus includes Python bindings with both synchronous and asynchronous APIs, built with PyO3.

### Installation

```bash
# From PyPI (once published)
pip install ztus

# Or build locally from source
pip install maturin
cd /path/to/ztus
maturin develop --release --features python
```

### Python 3.11+ Required

The Python bindings require Python 3.11 or later (due to PyO3 compatibility).

### Synchronous API

```python
import ztus

def progress_callback(bytes_transferred: int, total_bytes: int) -> None:
    """Progress callback function"""
    if total_bytes > 0:
        percent = (bytes_transferred / total_bytes) * 100
        print(f"Progress: {bytes_transferred}/{total_bytes} bytes ({percent:.1f}%)")

# Create client
client = ztus.TusClient()

# Upload with progress callback
client.upload(
    "/path/to/file.txt",
    "https://tus.server.tld/files",
    progress_callback=progress_callback
)

# Download file
client.download(
    "https://files.example.com/myfile.zip",
    "myfile.zip",
    progress_callback=progress_callback
)

# List incomplete uploads
incomplete = client.list_incomplete()

# Clean up old uploads (older than 7 days)
count = client.cleanup(days=7)
```

### Asynchronous API

```python
import asyncio
import ztus

async def main():
    def progress_callback(bytes_transferred: int, total_bytes: int) -> None:
        if total_bytes > 0:
            percent = (bytes_transferred / total_bytes) * 100
            print(f"Progress: {bytes_transferred}/{total_bytes} bytes ({percent:.1f}%)")

    client = ztus.TusClient()

    # Upload asynchronously
    await client.upload_async(
        "/path/to/file.txt",
        "https://tus.server.tld/files",
        progress_callback=progress_callback
    )

    # Resume interrupted upload
    await client.resume_async(
        "/path/to/file.txt",
        "https://tus.server.tld/files"
    )

    # Download asynchronously
    await client.download_async(
        "https://files.example.com/myfile.zip",
        "myfile.zip",
        progress_callback=progress_callback
    )

asyncio.run(main())
```

### Available Methods

| Method | Description | Sync | Async |
|--------|-------------|-----|-------|
| `upload(file_path, url, progress_callback=None)` | Upload a file | ✅ | ✅ |
| `resume(file_path, url, progress_callback=None)` | Resume interrupted upload | ✅ | ✅ |
| `download(url, output_path, chunk_size=None, progress_callback=None)` | Download a file | ✅ | ✅ |
| `list_incomplete()` | List incomplete uploads | ✅ | ❌ |
| `cleanup(days)` | Clean up old state | ✅ | ❌ |

### Progress Callback

The progress callback receives two integers:
- `bytes_transferred`: Total bytes transferred so far
- `total_bytes`: Total bytes to transfer (0 if unknown)

```python
def my_callback(bytes_transferred: int, total_bytes: int) -> None:
    print(f"{bytes_transferred} / {total_bytes}")
```

### Examples

See `python/examples/` for complete examples:
- `sync_upload.py` - Synchronous upload with progress
- `async_upload.py` - Asynchronous upload with progress
- `progress_callback.py` - Advanced progress tracking with ETA

## Performance

See [PERFORMANCE.md](PERFORMANCE.md) for detailed performance analysis and benchmarks.

### Summary

| Network Type | Throughput | 24 GB File Time | Improvement |
|--------------|------------|-----------------|-------------|
| Gigabit | 50-100+ MiB/s | ~5-10 min | **40-50x faster** |
| 100 Mbps | 20-25 MiB/s | ~16 min | **14-17x faster** |
| 25 Mbps | 4-5 MiB/s | ~1.3 hours | **3x faster** |

## Documentation

- [PERFORMANCE.md](PERFORMANCE.md) - Detailed performance analysis and optimization guide
- [CHANGELOG.md](CHANGELOG.md) - Version history and changes
- [TUS Protocol](https://tus.io/protocols/resumable-upload) - Protocol specification

## Configuration

ztus supports configuration via `~/.ztus/config.toml`:

```toml
[upload]
chunk_size = 5242880        # Fixed chunk size (if adaptive disabled)
max_retries = 3             # Maximum retry attempts
timeout = 30                # Request timeout (seconds)
verify_checksum = true      # Enable checksum verification
checksum_algorithm = "sha256"  # Checksum algorithm

[upload.adaptive]
enabled = true              # Enable adaptive chunk sizing
initial_chunk_size = 10485760  # 10 MB starting size
min_chunk_size = 5242880       # 5 MB minimum
max_chunk_size = 209715200     # 200 MB maximum
adaptation_interval = 5        # Evaluate every N chunks
stability_threshold = 0.1      # 10% variance threshold
```

## See Also

- [TUS Protocol](https://tus.io/protocols/resumable-upload)
- [zDownloadAPI](https://github.com/zachhandley/ZDownloadAPI) - High-performance TUS server with HTTP/2 support
