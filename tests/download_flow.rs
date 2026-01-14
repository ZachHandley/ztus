//! Integration tests for download functionality

use std::path::PathBuf;

#[tokio::test]
async fn test_download_state_creation() {
    use ztus::storage::DownloadState;

    let url = "https://example.com/file.bin".to_string();
    let output_path = PathBuf::from("/tmp/test.bin");
    let total_size = 1024 * 1024;
    let chunk_size = 512 * 1024;

    let state = DownloadState::new(url.clone(), output_path.clone(), total_size, chunk_size);

    assert!(!state.id.is_empty());
    assert_eq!(state.url, url);
    assert_eq!(state.output_path, output_path);
    assert_eq!(state.total_size, total_size);
    assert_eq!(state.offset, 0);
    assert_eq!(state.chunk_size, chunk_size);
    assert!(!state.is_complete());
    assert_eq!(state.progress(), 0.0);
}

#[tokio::test]
async fn test_download_state_progress() {
    use ztus::storage::DownloadState;

    let mut state = DownloadState::new(
        "https://example.com/file.bin".to_string(),
        PathBuf::from("/tmp/test.bin"),
        1000,
        512,
    );

    assert_eq!(state.progress(), 0.0);

    state.update_offset(500);
    assert_eq!(state.offset, 500);
    assert_eq!(state.progress(), 50.0);

    state.update_offset(1000);
    assert_eq!(state.progress(), 100.0);
    assert!(state.is_complete());
}

#[tokio::test]
async fn test_download_manager_creation() {
    use ztus::download::DownloadManager;

    let manager = DownloadManager::new(1024 * 1024);
    assert!(manager.is_ok());

    let manager = manager.unwrap();
    // Test builder pattern
    let _manager = manager.with_max_retries(5);
}

#[test]
fn test_range_header_parsing() {
    // Test various Range header formats
    let cases = vec![
        ("bytes=0-1023", "0", "1023"),
        ("bytes=1024-2047", "1024", "2047"),
        ("bytes=0-", "0", ""), // Open-ended
        ("bytes=100-100", "100", "100"), // Single byte
    ];

    for (header, expected_start, expected_end) in cases {
        let parts = header.strip_prefix("bytes=").unwrap();
        let (start, end) = parts.split_once('-').unwrap();
        assert_eq!(start, expected_start);
        assert_eq!(end, expected_end);
    }
}

#[tokio::test]
async fn test_http_status_codes() {
    // Test that we handle various HTTP status codes correctly
    use reqwest::StatusCode;

    // 200 OK - successful download
    assert!(StatusCode::OK.is_success());
    assert!(!StatusCode::OK.is_client_error());

    // 206 Partial Content - successful range request
    assert!(StatusCode::PARTIAL_CONTENT.is_success());
    assert_eq!(StatusCode::PARTIAL_CONTENT.as_u16(), 206);

    // 416 Range Not Satisfiable - invalid range
    assert!(StatusCode::RANGE_NOT_SATISFIABLE.is_client_error());
    assert_eq!(StatusCode::RANGE_NOT_SATISFIABLE.as_u16(), 416);

    // 404 Not Found
    assert!(StatusCode::NOT_FOUND.is_client_error());

    // 500 Internal Server Error
    assert!(StatusCode::INTERNAL_SERVER_ERROR.is_server_error());
}

#[tokio::test]
async fn test_download_error_handling() {
    use ztus::error::ZtusError;

    // Test error conversion
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let ztus_error: ZtusError = io_error.into();
    assert!(matches!(ztus_error, ZtusError::IoError(_)));

    // Test protocol error
    let protocol_error = ZtusError::ProtocolError("test error".to_string());
    assert_eq!(protocol_error.to_string(), "TUS protocol error: test error");
}

#[test]
fn test_download_state_display() {
    use ztus::storage::DownloadState;

    let state = DownloadState::new(
        "https://example.com/file.bin".to_string(),
        PathBuf::from("/tmp/test.bin"),
        1000,
        512,
    );

    let display = format!("{}", state);
    assert!(display.contains("https://example.com/file.bin"));
    assert!(display.contains("/tmp/test.bin"));
    assert!(display.contains("0.0%"));

    let mut state = state;
    state.update_offset(500);
    let display = format!("{}", state);
    assert!(display.contains("50.0%"));
}
