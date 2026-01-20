//! Integration tests for upload flow

use std::path::PathBuf;
use std::time::Duration;
use ztus::client::TusClient;
use ztus::config::UploadConfig;
use ztus::protocol::{TusExtension, TusProtocol, ServerCapabilities};
use ztus::storage::{StateStorage, UploadState};

#[test]
fn test_state_storage() {
    let temp_dir = PathBuf::from("/tmp/ztus-test");
    let _ = std::fs::remove_dir_all(&temp_dir);

    let storage = StateStorage::new(temp_dir.clone()).unwrap();

    // Create a test state (UploadState::new takes file_path, upload_url, file_size)
    let state = UploadState::new(
        PathBuf::from("/tmp/test.txt"),
        "http://example.com/upload/123".to_string(),
        1024,
    );

    // Save state
    storage.save_state(&state).unwrap();

    // Load state
    let loaded = storage.load_state(&state.id).unwrap();
    assert_eq!(loaded.id, state.id);
    assert_eq!(loaded.file_size, state.file_size);

    // List states
    let states = storage.list_states().unwrap();
    assert_eq!(states.len(), 1);

    // Delete state
    storage.delete_state(&state.id).unwrap();
    let states = storage.list_states().unwrap();
    assert_eq!(states.len(), 0);

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_upload_state_progress() {
    let state = UploadState::new(
        PathBuf::from("/tmp/test.txt"),
        "http://example.com/upload/123".to_string(),
        1024,
    );

    assert_eq!(state.progress(), 0.0);
    assert!(!state.is_complete());

    let mut updated = state.clone();
    updated.update_offset(512);
    assert_eq!(updated.progress(), 50.0);

    updated.update_offset(1024);
    assert_eq!(updated.progress(), 100.0);
    assert!(updated.is_complete());
}

#[test]
fn test_protocol_client_creation() {
    let client = TusProtocol::new(
        "http://example.com".to_string(),
        Duration::from_secs(30),
    );
    assert!(client.is_ok());
}

#[test]
fn test_upload_config_validation() {
    let config = UploadConfig::new();
    assert!(config.validate().is_ok());

    // Test invalid chunk size
    let mut invalid = config.clone();
    invalid.chunk_size = 0;
    assert!(invalid.validate().is_err());

    // Test too large chunk size
    let mut too_large = config;
    too_large.chunk_size = 2 * 1024 * 1024 * 1024; // 2GB
    assert!(too_large.validate().is_err());
}

#[test]
fn test_server_capabilities_display() {
    let capabilities = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![
            TusExtension::Creation,
            TusExtension::Termination,
            TusExtension::Checksum,
        ],
        max_size: Some(5 * 1024 * 1024 * 1024), // 5GB
    };

    let display = format!("{}", capabilities);
    assert!(display.contains("TUS Server Capabilities"));
    assert!(display.contains("1.0.0"));
    assert!(display.contains("creation"));
    assert!(display.contains("termination"));
    assert!(display.contains("checksum"));
    // Check for GB formatting (5GB in bytes = ~4.88 GiB when divided by 1024^3)
    assert!(display.contains("GB") || display.contains("bytes"));
}

#[test]
fn test_server_capabilities_no_extensions() {
    let capabilities = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![],
        max_size: None,
    };

    let display = format!("{}", capabilities);
    assert!(display.contains("Extensions: None"));
    assert!(display.contains("Not specified by server"));
}

#[test]
fn test_client_creation() {
    let client = TusClient::new();
    assert!(client.is_ok());
}

#[test]
fn test_tus_extension_parsing() {
    // Test from_str (via FromStr trait)
    assert_eq!("creation".parse::<TusExtension>(), Ok(TusExtension::Creation));
    assert_eq!("termination".parse::<TusExtension>(), Ok(TusExtension::Termination));
    assert_eq!("checksum".parse::<TusExtension>(), Ok(TusExtension::Checksum));
    assert_eq!("expiration".parse::<TusExtension>(), Ok(TusExtension::Expiration));
    assert_eq!("concatenation".parse::<TusExtension>(), Ok(TusExtension::Concatenation));
    assert_eq!("creation-with-upload".parse::<TusExtension>(), Ok(TusExtension::CreationWithUpload));
    assert!("invalid".parse::<TusExtension>().is_err());

    // Test as_str
    assert_eq!(TusExtension::Creation.as_str(), "creation");
    assert_eq!(TusExtension::Termination.as_str(), "termination");
    assert_eq!(TusExtension::Checksum.as_str(), "checksum");
}
