//! Integration tests for TUS protocol features
//!
//! Tests for discover_capabilities and terminate_upload functionality.

use ztus::protocol::{ServerCapabilities, TusExtension};

#[test]
fn test_server_capabilities_extensions_equality() {
    // Test that extension comparison works correctly
    let ext1 = TusExtension::Creation;
    let ext2 = TusExtension::Creation;
    let ext3 = TusExtension::Termination;

    assert_eq!(ext1, ext2);
    assert_ne!(ext1, ext3);
}

#[test]
fn test_server_capabilities_contains_extension() {
    let capabilities = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![
            TusExtension::Creation,
            TusExtension::Termination,
            TusExtension::Checksum,
        ],
        max_size: Some(1024 * 1024 * 1024),
    };

    assert!(capabilities.extensions.contains(&TusExtension::Creation));
    assert!(capabilities.extensions.contains(&TusExtension::Termination));
    assert!(capabilities.extensions.contains(&TusExtension::Checksum));
    assert!(!capabilities.extensions.contains(&TusExtension::Expiration));
}

#[test]
fn test_server_capabilities_max_size_check() {
    let capabilities_with_limit = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![],
        max_size: Some(100 * 1024 * 1024), // 100MB
    };

    let capabilities_no_limit = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![],
        max_size: None,
    };

    // Test with limit
    if let Some(max) = capabilities_with_limit.max_size {
        assert!(max > 0);
        assert_eq!(max, 100 * 1024 * 1024);
    }

    // Test without limit
    assert!(capabilities_no_limit.max_size.is_none());
}

#[test]
fn test_tus_extension_display_names() {
    assert_eq!(TusExtension::Creation.as_str(), "creation");
    assert_eq!(TusExtension::CreationWithUpload.as_str(), "creation-with-upload");
    assert_eq!(TusExtension::Termination.as_str(), "termination");
    assert_eq!(TusExtension::Checksum.as_str(), "checksum");
    assert_eq!(TusExtension::Expiration.as_str(), "expiration");
    assert_eq!(TusExtension::Concatenation.as_str(), "concatenation");
}

#[test]
fn test_tus_extension_roundtrip() {
    // Test individual extensions to avoid borrow checker issues
    let test_cases = vec![
        TusExtension::Creation,
        TusExtension::CreationWithUpload,
        TusExtension::Termination,
        TusExtension::Checksum,
        TusExtension::Expiration,
        TusExtension::Concatenation,
    ];

    for ext in test_cases {
        let str_repr = ext.as_str();

        // Verify as_str matches expected value
        match ext {
            TusExtension::Creation => assert_eq!(str_repr, "creation"),
            TusExtension::CreationWithUpload => assert_eq!(str_repr, "creation-with-upload"),
            TusExtension::Termination => assert_eq!(str_repr, "termination"),
            TusExtension::Checksum => assert_eq!(str_repr, "checksum"),
            TusExtension::Expiration => assert_eq!(str_repr, "expiration"),
            TusExtension::Concatenation => assert_eq!(str_repr, "concatenation"),
        }

        // Test roundtrip by cloning to avoid borrow checker
        let ext_clone = ext.clone();
        let parsed = str_repr.parse::<TusExtension>();
        assert_eq!(parsed, Ok(ext_clone), "Failed roundtrip for: {}", str_repr);
    }
}

#[test]
fn test_server_capabilities_formatting() {
    let capabilities = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![
            TusExtension::Creation,
            TusExtension::Termination,
        ],
        max_size: Some(512 * 1024 * 1024), // 512MB
    };

    let output = format!("{}", capabilities);

    // Check that all expected sections are present
    assert!(output.contains("TUS Server Capabilities"));
    assert!(output.contains("Protocol Version"));
    assert!(output.contains("1.0.0"));
    assert!(output.contains("Supported Extensions"));
    assert!(output.contains("creation"));
    assert!(output.contains("termination"));
    assert!(output.contains("Maximum Upload Size"));
}

#[test]
fn test_server_capabilities_empty_extensions() {
    let capabilities = ServerCapabilities {
        version: "0.9.0".to_string(),
        extensions: vec![],
        max_size: None,
    };

    let output = format!("{}", capabilities);

    assert!(output.contains("Extensions: None"));
    assert!(output.contains("Not specified by server"));
}

#[test]
fn test_server_capabilities_large_file_size() {
    let capabilities = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![],
        max_size: Some(5 * 1024 * 1024 * 1024), // 5GB
    };

    let output = format!("{}", capabilities);

    // Should display in GB
    assert!(output.contains("5.00 GB") || output.contains("5 GB"));
}

#[test]
fn test_server_capabilities_medium_file_size() {
    let capabilities = ServerCapabilities {
        version: "1.0.0".to_string(),
        extensions: vec![],
        max_size: Some(10 * 1024 * 1024), // 10MB
    };

    let output = format!("{}", capabilities);

    // Should display in MB
    assert!(output.contains("10.00 MB") || output.contains("10 MB"));
}
