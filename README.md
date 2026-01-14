# ztus

ztus (like yeetus but with a z) - a blazingly fast Rust CLI tool for resumable uploads using the TUS protocol and chunked downloads.

## About

A TUS (tus.io) client because existing ones were out of date or abandoned. Implements the [TUS resumable upload protocol](https://tus.io/protocols/resumable-upload) for reliable file uploads that can be interrupted and resumed, plus wget/curl-like chunked downloads.

## Features

- **Resumable Uploads**: Interrupt and resume uploads at any point
- **Chunked Transfers**: Efficient handling of large files with configurable chunk sizes
- **Progress Reporting**: Real-time progress bars with speed and ETA
- **Checksum Verification**: SHA1/SHA256 support for data integrity
- **HTTP Range Downloads**: wget/curl-like downloads with resume support
- **Cross-Platform**: Native binaries for Linux, macOS (amd64/arm64), and Windows
- **State Persistence**: Automatic resume state storage in `~/.ztus/`

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
# Basic upload
ztus upload myfile.zip https://files.example.com/upload/

# Upload with custom chunk size (default: 5MB)
ztus upload myfile.zip https://files.example.com/upload/ -c 10485760

# Resume interrupted upload
ztus resume myfile.zip https://files.example.com/upload/
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

## See Also

- [TUS Protocol](https://tus.io/protocols/resumable-upload)
