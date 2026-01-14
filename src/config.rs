//! Configuration management for ztus
//!
//! This module handles configuration loading and validation.

use crate::error::{Result, ZtusError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Default chunk size for uploads (5 MB)
pub const DEFAULT_CHUNK_SIZE: usize = 5 * 1024 * 1024;

/// Default TUS protocol version
pub const DEFAULT_TUS_VERSION: &str = "1.0.0";

/// Default initial chunk size for adaptive uploads (10 MB)
pub const DEFAULT_ADAPTIVE_INITIAL_CHUNK_SIZE: usize = 10 * 1024 * 1024;

/// Default minimum chunk size for adaptive uploads (1 MB)
pub const DEFAULT_ADAPTIVE_MIN_CHUNK_SIZE: usize = 1 * 1024 * 1024;

/// Default maximum chunk size for adaptive uploads (200 MB)
pub const DEFAULT_ADAPTIVE_MAX_CHUNK_SIZE: usize = 200 * 1024 * 1024;

/// Default adaptation interval for adaptive uploads (5 chunks)
pub const DEFAULT_ADAPTATION_INTERVAL: usize = 5;

/// Default stability threshold for adaptive uploads (10%)
pub const DEFAULT_STABILITY_THRESHOLD: f64 = 0.1;

/// Configuration for adaptive chunk sizing during uploads
///
/// Adaptive chunk sizing allows the uploader to dynamically adjust chunk sizes
/// based on network performance, optimizing for both speed and reliability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveChunkConfig {
    /// Whether adaptive chunk sizing is enabled
    #[serde(default = "default_adaptive_enabled")]
    pub enabled: bool,

    /// Initial chunk size to start with (in bytes)
    #[serde(default = "default_adaptive_initial_chunk_size")]
    pub initial_chunk_size: usize,

    /// Minimum chunk size allowed (in bytes)
    #[serde(default = "default_adaptive_min_chunk_size")]
    pub min_chunk_size: usize,

    /// Maximum chunk size allowed (in bytes)
    #[serde(default = "default_adaptive_max_chunk_size")]
    pub max_chunk_size: usize,

    /// Number of chunks to process before re-evaluating chunk size
    #[serde(default = "default_adaptation_interval")]
    pub adaptation_interval: usize,

    /// Variance threshold for considering upload speeds stable (0.0-1.0)
    ///
    /// Lower values mean the system will only adjust chunk sizes when
    /// upload speeds are very consistent. Default is 0.1 (10% variance).
    #[serde(default = "default_stability_threshold")]
    pub stability_threshold: f64,
}

impl Default for AdaptiveChunkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_chunk_size: DEFAULT_ADAPTIVE_INITIAL_CHUNK_SIZE,
            min_chunk_size: DEFAULT_ADAPTIVE_MIN_CHUNK_SIZE,
            max_chunk_size: DEFAULT_ADAPTIVE_MAX_CHUNK_SIZE,
            adaptation_interval: DEFAULT_ADAPTATION_INTERVAL,
            stability_threshold: DEFAULT_STABILITY_THRESHOLD,
        }
    }
}

fn default_adaptive_enabled() -> bool {
    true
}

fn default_adaptive_initial_chunk_size() -> usize {
    DEFAULT_ADAPTIVE_INITIAL_CHUNK_SIZE
}

fn default_adaptive_min_chunk_size() -> usize {
    DEFAULT_ADAPTIVE_MIN_CHUNK_SIZE
}

fn default_adaptive_max_chunk_size() -> usize {
    DEFAULT_ADAPTIVE_MAX_CHUNK_SIZE
}

fn default_adaptation_interval() -> usize {
    DEFAULT_ADAPTATION_INTERVAL
}

fn default_stability_threshold() -> f64 {
    DEFAULT_STABILITY_THRESHOLD
}

/// Configuration for TUS uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadConfig {
    /// Chunk size in bytes
    pub chunk_size: usize,

    /// TUS protocol version
    pub tus_version: String,

    /// Maximum number of retry attempts
    pub max_retries: usize,

    /// Request timeout in seconds
    pub timeout: u64,

    /// Whether to resume existing uploads when state is found
    #[serde(default = "default_resume")]
    pub resume: bool,

    /// Verbose output - show detailed upload progress
    #[serde(default)]
    pub verbose: bool,

    /// Custom HTTP headers to include in requests
    pub headers: Vec<(String, String)>,

    /// Metadata to attach to upload
    pub metadata: Vec<(String, String)>,

    /// Whether to verify checksums
    pub verify_checksum: bool,

    /// Checksum algorithm to use
    pub checksum_algorithm: ChecksumAlgorithm,

    /// Adaptive chunk sizing configuration
    #[serde(default)]
    pub adaptive: AdaptiveChunkConfig,
}

/// Checksum algorithm options
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    Sha1,
    Sha256,
}

impl ChecksumAlgorithm {
    /// Get the algorithm name as used in TUS protocol
    pub fn as_tus_algorithm(&self) -> &str {
        match self {
            ChecksumAlgorithm::Sha1 => "sha1",
            ChecksumAlgorithm::Sha256 => "sha256",
        }
    }
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            tus_version: DEFAULT_TUS_VERSION.to_string(),
            max_retries: 3,
            timeout: 30,
            resume: true,
            verbose: false,
            headers: Vec::new(),
            metadata: Vec::new(),
            verify_checksum: true,
            checksum_algorithm: ChecksumAlgorithm::Sha256,
            adaptive: AdaptiveChunkConfig::default(),
        }
    }
}

fn default_resume() -> bool {
    true
}

impl UploadConfig {
    /// Create a new upload config with defaults
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set chunk size
    #[allow(dead_code)]
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set max retries
    #[allow(dead_code)]
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Add a custom header
    #[allow(dead_code)]
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.push((key, value));
        self
    }

    /// Add metadata
    #[allow(dead_code)]
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.push((key, value));
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.chunk_size == 0 {
            return Err(ZtusError::ConfigError("Chunk size cannot be zero".into()));
        }

        if self.chunk_size > 1024 * 1024 * 1024 {
            return Err(ZtusError::ConfigError(
                "Chunk size too large (max 1GB)".into(),
            ));
        }

        if self.max_retries > 10 {
            return Err(ZtusError::ConfigError(
                "Max retries too high (max 10)".into(),
            ));
        }

        Ok(())
    }
}

/// Global application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// State directory path
    pub state_dir: PathBuf,

    /// Upload configuration defaults
    pub upload: UploadConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let state_dir = dirs::home_dir()
            .map(|p| p.join(".ztus"))
            .unwrap_or_else(|| PathBuf::from(".ztus"));

        Self {
            state_dir,
            upload: UploadConfig::default(),
        }
    }
}

impl AppConfig {
    /// Get the path to the config file
    pub fn config_path() -> PathBuf {
        Self::default().state_dir.join("config.toml")
    }

    /// Load configuration from default location
    ///
    /// Returns default configuration if the config file doesn't exist.
    /// Merges partial configs with defaults.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        // Return default config if file doesn't exist
        if !config_path.exists() {
            tracing::debug!("No config file found at {}, using defaults", config_path.display());
            return Ok(Self::default());
        }

        // Read and parse the config file
        let contents = fs::read_to_string(&config_path).map_err(|e| {
            ZtusError::ConfigError(format!("Failed to read config file {}: {}", config_path.display(), e))
        })?;

        let mut config: AppConfig = toml::from_str(&contents).map_err(|e| {
            ZtusError::ConfigError(format!("Failed to parse config file {}: {}", config_path.display(), e))
        })?;

        // Ensure state_dir is set correctly (override from config file if needed)
        config.state_dir = Self::default().state_dir;

        Ok(config)
    }

    /// Save configuration to default location
    ///
    /// Creates the config directory if it doesn't exist.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();

        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ZtusError::ConfigError(format!(
                    "Failed to create config directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Serialize to TOML
        let toml_string = toml::to_string_pretty(self).map_err(|e| {
            ZtusError::ConfigError(format!("Failed to serialize config: {}", e))
        })?;

        // Write to file with a temporary file for atomicity
        let temp_path = config_path.with_extension("tmp");
        {
            let mut file = fs::File::create(&temp_path).map_err(|e| {
                ZtusError::ConfigError(format!("Failed to create temp file {}: {}", temp_path.display(), e))
            })?;

            file.write_all(toml_string.as_bytes()).map_err(|e| {
                ZtusError::ConfigError(format!("Failed to write config: {}", e))
            })?;

            file.flush().map_err(|e| {
                ZtusError::ConfigError(format!("Failed to flush config: {}", e))
            })?;
        }

        // Atomic rename
        fs::rename(&temp_path, &config_path).map_err(|e| {
            ZtusError::ConfigError(format!(
                "Failed to rename {} to {}: {}",
                temp_path.display(),
                config_path.display(),
                e
            ))
        })?;

        tracing::debug!("Configuration saved to {}", config_path.display());
        Ok(())
    }

    /// Get a config value by key path (e.g., "upload.chunk_size")
    pub fn get_value(&self, key: &str) -> Result<String> {
        let parts: Vec<&str> = key.split('.').collect();

        match parts.as_slice() {
            ["state_dir"] => Ok(self.state_dir.display().to_string()),
            ["upload", "chunk_size"] => Ok(self.upload.chunk_size.to_string()),
            ["upload", "tus_version"] => Ok(self.upload.tus_version.clone()),
            ["upload", "max_retries"] => Ok(self.upload.max_retries.to_string()),
            ["upload", "timeout"] => Ok(self.upload.timeout.to_string()),
            ["upload", "verify_checksum"] => Ok(self.upload.verify_checksum.to_string()),
            ["upload", "checksum_algorithm"] => Ok(format!("{:?}", self.upload.checksum_algorithm)),
            _ => Err(ZtusError::ConfigError(format!("Unknown config key: {}", key))),
        }
    }

    /// Set a config value by key path (e.g., "upload.chunk_size")
    pub fn set_value(&mut self, key: &str, value: &str) -> Result<()> {
        let parts: Vec<&str> = key.split('.').collect();

        match parts.as_slice() {
            ["upload", "chunk_size"] => {
                self.upload.chunk_size = value.parse().map_err(|_| {
                    ZtusError::ConfigError(format!("Invalid chunk size: {}", value))
                })?;
            }
            ["upload", "max_retries"] => {
                self.upload.max_retries = value.parse().map_err(|_| {
                    ZtusError::ConfigError(format!("Invalid max retries: {}", value))
                })?;
            }
            ["upload", "timeout"] => {
                self.upload.timeout = value.parse().map_err(|_| {
                    ZtusError::ConfigError(format!("Invalid timeout: {}", value))
                })?;
            }
            ["upload", "verify_checksum"] => {
                self.upload.verify_checksum = value.parse().map_err(|_| {
                    ZtusError::ConfigError(format!("Invalid boolean: {}", value))
                })?;
            }
            ["upload", "checksum_algorithm"] => {
                self.upload.checksum_algorithm = match value.to_lowercase().as_str() {
                    "sha1" => ChecksumAlgorithm::Sha1,
                    "sha256" => ChecksumAlgorithm::Sha256,
                    _ => {
                        return Err(ZtusError::ConfigError(format!(
                            "Invalid checksum algorithm: {}. Must be 'sha1' or 'sha256'",
                            value
                        )))
                    }
                };
            }
            _ => {
                return Err(ZtusError::ConfigError(format!(
                    "Unknown or read-only config key: {}",
                    key
                )))
            }
        }

        // Validate after setting
        self.upload.validate()?;

        Ok(())
    }
}
