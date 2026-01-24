//! Error types for ztus
//!
//! This module defines all error types using thiserror for structured error handling.

use std::io;
use thiserror::Error;

/// Main error type for ztus
#[derive(Error, Debug)]
pub enum ZtusError {
    /// HTTP client errors
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    /// IO errors
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    /// TUS protocol errors
    #[error("TUS protocol error: {0}")]
    ProtocolError(String),

    /// TUS protocol version not supported
    #[error("Unsupported TUS version: {0}")]
    #[allow(dead_code)]
    UnsupportedVersion(String),

    /// Server does not support required TUS extension
    #[error("Server does not support required extension: {0}")]
    #[allow(dead_code)]
    MissingExtension(String),

    /// Checksum mismatch
    #[error("Checksum verification failed: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// Invalid upload state
    #[error("Invalid upload state: {0}")]
    InvalidState(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    #[allow(dead_code)]
    InvalidUrl(String),

    /// Upload terminated by server
    #[error("Upload terminated by server")]
    UploadTerminated,

    /// Offset mismatch
    #[error("Offset mismatch: expected {expected}, got {actual}")]
    OffsetMismatch { expected: u64, actual: u64 },
}

/// Result type alias for ztus
pub type Result<T> = std::result::Result<T, ZtusError>;
