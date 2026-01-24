# ztus

A blazingly fast TUS client for resumable uploads and chunked downloads, built in Rust with Python bindings.

**Performance**: Achieves **50-100+ MiB/s** on gigabit networks with adaptive chunk sizing and HTTP/2 support - a **40-50x improvement** over traditional TUS clients.

## Installation

```bash
pip install ztus
```

**Requires Python 3.11**

## Quick Start

```python
import ztus

client = ztus.TusClient()

# Upload a file
client.upload("/path/to/file.zip", "https://tus.server.tld/files")

# Download a file
client.download("https://files.example.com/file.zip", "output.zip")
```

## Features

- **Resumable Uploads**: TUS protocol support with automatic resume on interruption
- **Adaptive Chunk Sizing**: Automatically optimizes chunk sizes (1 MB - 200 MB) based on network conditions
- **HTTP/2 Support**: Leverages HTTP/2 multiplexing when available
- **Progress Callbacks**: Real-time progress tracking for uploads and downloads
- **Sync and Async APIs**: Use synchronous or asynchronous methods based on your needs
- **State Persistence**: Automatic resume state storage for crash recovery

## Synchronous API

```python
import ztus

def progress_callback(bytes_transferred: int, total_bytes: int) -> None:
    if total_bytes > 0:
        percent = (bytes_transferred / total_bytes) * 100
        print(f"Progress: {percent:.1f}%")

client = ztus.TusClient()

# Upload with progress tracking
client.upload(
    "/path/to/file.txt",
    "https://tus.server.tld/files",
    progress_callback=progress_callback
)

# Resume an interrupted upload
client.resume(
    "/path/to/file.txt",
    "https://tus.server.tld/files",
    progress_callback=progress_callback
)

# Download with progress tracking
client.download(
    "https://files.example.com/file.zip",
    "output.zip",
    chunk_size=10 * 1024 * 1024,  # 10 MB chunks
    progress_callback=progress_callback
)

# List incomplete uploads
incomplete = client.list_incomplete()

# Clean up uploads older than 7 days
cleaned = client.cleanup(days=7)
```

## Asynchronous API

```python
import asyncio
import ztus

async def main():
    def progress_callback(bytes_transferred: int, total_bytes: int) -> None:
        if total_bytes > 0:
            print(f"Progress: {(bytes_transferred / total_bytes) * 100:.1f}%")

    client = ztus.TusClient()

    # Async upload
    await client.upload_async(
        "/path/to/file.txt",
        "https://tus.server.tld/files",
        progress_callback=progress_callback
    )

    # Async resume
    await client.resume_async(
        "/path/to/file.txt",
        "https://tus.server.tld/files"
    )

    # Async download
    await client.download_async(
        "https://files.example.com/file.zip",
        "output.zip",
        progress_callback=progress_callback
    )

asyncio.run(main())
```

## API Reference

### TusClient

The main client class for all operations.

```python
client = ztus.TusClient()
```

### Methods

| Method | Description |
|--------|-------------|
| `upload(file_path, url, progress_callback=None)` | Upload a file synchronously |
| `upload_async(file_path, url, progress_callback=None)` | Upload a file asynchronously |
| `resume(file_path, url, progress_callback=None)` | Resume an interrupted upload synchronously |
| `resume_async(file_path, url, progress_callback=None)` | Resume an interrupted upload asynchronously |
| `download(url, output_path, chunk_size=None, progress_callback=None)` | Download a file synchronously |
| `download_async(url, output_path, chunk_size=None, progress_callback=None)` | Download a file asynchronously |
| `list_incomplete()` | List incomplete uploads (returns list of strings) |
| `cleanup(days)` | Clean up uploads older than N days (returns count) |

### Progress Callback

The progress callback receives two integers:

- `bytes_transferred`: Total bytes transferred so far
- `total_bytes`: Total bytes to transfer (0 if unknown)

```python
def my_callback(bytes_transferred: int, total_bytes: int) -> None:
    # Your progress handling logic
    pass
```

## Performance

| Network | Throughput | 24 GB File |
|---------|------------|------------|
| Gigabit | 50-100+ MiB/s | ~5-10 min |
| 100 Mbps | 20-25 MiB/s | ~16 min |
| 25 Mbps | 4-5 MiB/s | ~1.3 hours |

## Links

- [GitHub Repository](https://github.com/zachhandley/ztus)
- [TUS Protocol](https://tus.io/protocols/resumable-upload)
- [Issue Tracker](https://github.com/zachhandley/ztus/issues)

## License

MIT OR Apache-2.0
