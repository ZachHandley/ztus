//! ztus - A blazingly fast CLI tool for resumable uploads using the TUS protocol
//!
//! This is the main entry point for the ztus command-line interface.

mod checksum;
mod client;
mod config;
mod download;
mod error;
mod protocol;
mod storage;
mod upload;

use clap::{Parser, Subcommand, ValueEnum};
use client::TusClient;
use config::{AppConfig, ChecksumAlgorithm};
use error::Result;
use std::collections::HashMap;

/// Checksum algorithm for upload verification
#[derive(Clone, Debug, ValueEnum)]
pub enum ChecksumArg {
    /// SHA-1 checksum
    Sha1,

    /// SHA-256 checksum
    Sha256,

    /// Disable checksum verification
    None,
}

#[derive(Parser)]
#[command(name = "ztus")]
#[command(about = "A blazingly fast CLI tool for resumable uploads using the TUS protocol", long_about = None)]
#[command(version = "0.1.0")]
#[command(author = "Zach Handley <zachhandley@gmail.com>")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Upload a file to a TUS endpoint
    Upload {
        /// Path to the file to upload
        file: String,

        /// TUS upload endpoint URL
        url: String,

        /// Chunk size in bytes (default: 5MB)
        #[arg(short, long)]
        chunk_size: Option<usize>,

        /// Maximum number of retry attempts (default: 3)
        #[arg(short, long)]
        max_retries: Option<usize>,

        /// Checksum algorithm to use
        #[arg(long, value_name = "ALGO")]
        checksum: Option<ChecksumArg>,

        /// Disable checksum verification
        #[arg(long)]
        no_checksum: bool,

        /// Upload metadata in key:value format (can be used multiple times)
        /// Values will be base64-encoded per TUS specification
        #[arg(long, value_name = "KEY:VALUE")]
        metadata: Vec<String>,

        /// Custom HTTP headers in key:value format (can be used multiple times)
        #[arg(long, alias = "headers", value_name = "KEY:VALUE")]
        header: Vec<String>,
    },

    /// Resume an incomplete upload
    Resume {
        /// Path to the file to upload
        file: String,

        /// TUS upload endpoint URL
        url: String,

        /// Custom HTTP headers in key:value format (can be used multiple times)
        #[arg(long, alias = "headers", value_name = "KEY:VALUE")]
        header: Vec<String>,
    },

    /// Download a file from a URL
    Download {
        /// URL to download from
        url: String,

        /// Output file path
        #[arg(short, long)]
        output: String,

        /// Chunk size in bytes (default: 5MB)
        #[arg(short, long)]
        chunk_size: Option<usize>,
    },

    /// List incomplete uploads
    List,

    /// Clean up old incomplete uploads
    Cleanup {
        /// Number of days of incomplete uploads to keep (default: 7)
        #[arg(short, long, default_value = "7")]
        days: i64,
    },

    /// Show configuration and state directory
    Info {
        /// TUS server URL to query for capabilities
        url: Option<String>,
    },

    /// Terminate an upload at the given URL
    Terminate {
        /// Upload URL to terminate
        upload_url: String,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Get a configuration value
    Get {
        /// Configuration key (e.g., "upload.chunk_size")
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Configuration key (e.g., "upload.chunk_size")
        key: String,

        /// New value
        value: String,
    },

    /// List all configuration values
    List,

    /// Open configuration file in editor
    Edit,
}

/// Parse metadata from command line arguments
///
/// Accepts formats:
/// - "key:value" - single key-value pair
///
/// Returns a HashMap of key-value pairs
fn parse_metadata(metadata_args: &[String]) -> Result<HashMap<String, String>> {
    let mut metadata = HashMap::new();

    for arg in metadata_args {
        // Try to parse as "key:value" format
        if let Some((key, value)) = arg.split_once(':') {
            if key.is_empty() {
                return Err(error::ZtusError::ConfigError(
                    "Metadata key cannot be empty".to_string()
                ));
            }
            if value.is_empty() {
                return Err(error::ZtusError::ConfigError(
                    format!("Metadata value for key '{}' cannot be empty", key)
                ));
            }
            metadata.insert(key.to_string(), value.to_string());
        } else {
            // If no colon found, treat the entire string as a key with empty value
            // This is an error condition
            return Err(error::ZtusError::ConfigError(
                format!("Invalid metadata format '{}'. Expected 'key:value'", arg)
            ));
        }
    }

    Ok(metadata)
}

/// Parse custom headers from command line arguments
///
/// Accepts formats:
/// - "key:value" - single key-value pair
/// - "key: value" - key-value pair with space (space will be trimmed)
///
/// Returns a Vec of key-value tuples
fn parse_headers(header_args: &[String]) -> Result<Vec<(String, String)>> {
    let mut headers = Vec::new();

    for arg in header_args {
        // Try to parse as "key:value" format
        if let Some((key, value)) = arg.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            if key.is_empty() {
                return Err(error::ZtusError::ConfigError(
                    "Header key cannot be empty".to_string()
                ));
            }
            if value.is_empty() {
                return Err(error::ZtusError::ConfigError(
                    format!("Header value for key '{}' cannot be empty", key)
                ));
            }
            headers.push((key.to_string(), value.to_string()));
        } else {
            // If no colon found, this is an error
            return Err(error::ZtusError::ConfigError(
                format!("Invalid header format '{}'. Expected 'key:value'", arg)
            ));
        }
    }

    Ok(headers)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let log_level = if cli.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .init();

    // Create TUS client
    let client = TusClient::new()?;

    // Execute command
    match cli.command {
        Commands::Upload {
            file,
            url,
            chunk_size,
            max_retries,
            checksum,
            no_checksum,
            metadata,
            header,
        } => {
            tracing::info!("Uploading {} to {}", file, url);

            let file_path = std::path::Path::new(&file);

            let mut config = client.upload_config().clone();

            if let Some(size) = chunk_size {
                config.chunk_size = size;
            }

            if let Some(retries) = max_retries {
                config.max_retries = retries;
            }

            // Handle checksum configuration
            if no_checksum {
                config.verify_checksum = false;
            } else if let Some(checksum_arg) = checksum {
                match checksum_arg {
                    ChecksumArg::Sha1 => {
                        config.verify_checksum = true;
                        config.checksum_algorithm = ChecksumAlgorithm::Sha1;
                    }
                    ChecksumArg::Sha256 => {
                        config.verify_checksum = true;
                        config.checksum_algorithm = ChecksumAlgorithm::Sha256;
                    }
                    ChecksumArg::None => {
                        config.verify_checksum = false;
                    }
                }
            }

            // Parse and set metadata
            if !metadata.is_empty() {
                let parsed_metadata = parse_metadata(&metadata)?;
                tracing::debug!("Parsed metadata: {:?}", parsed_metadata);
                config.metadata = parsed_metadata.into_iter().collect();
            }

            // Parse and set custom headers
            if !header.is_empty() {
                let parsed_headers = parse_headers(&header)?;
                tracing::debug!("Parsed headers: {:?}", parsed_headers);
                config.headers = parsed_headers;
            }

            // Validate config
            config.validate()?;

            client.upload_with_config(file_path, &url, &config).await?;
        }

        Commands::Resume { file, url, header } => {
            tracing::info!("Resuming upload {} to {}", file, url);

            let file_path = std::path::Path::new(&file);
            let mut config = client.upload_config().clone();

            // Parse and set custom headers
            if !header.is_empty() {
                let parsed_headers = parse_headers(&header)?;
                tracing::debug!("Parsed headers: {:?}", parsed_headers);
                config.headers = parsed_headers;
            }

            client.resume_with_config(file_path, &url, &config).await?;
        }

        Commands::Download {
            url,
            output,
            chunk_size,
        } => {
            tracing::info!("Downloading {} to {}", url, output);

            let output_path = std::path::Path::new(&output);

            let size = chunk_size.unwrap_or_else(|| client.upload_config().chunk_size);

            client.download_with_chunk_size(&url, output_path, size).await?;
        }

        Commands::List => {
            println!("Incomplete uploads:");
            let incomplete = client.list_incomplete()?;

            if incomplete.is_empty() {
                println!("  No incomplete uploads found");
            } else {
                for upload in incomplete {
                    println!("  - {}", upload);
                }
            }
        }

        Commands::Cleanup { days } => {
            tracing::info!("Cleaning up uploads older than {} days", days);

            let cleaned = client.cleanup(days)?;

            if cleaned == 0 {
                println!("No old uploads to clean up");
            } else {
                println!("Cleaned up {} old upload(s)", cleaned);
            }
        }

        Commands::Info { url } => {
            if let Some(target_url) = url {
                // Check if this looks like an upload URL or server URL
                // Upload URLs typically have path segments after the endpoint
                // Server URLs are typically just the base endpoint
                let is_upload_url = target_url.matches('/').count() > 3
                    || target_url.contains("/files/")
                    || target_url.contains("/uploads/")
                    || target_url.contains("/upload/");

                if is_upload_url {
                    // Query upload information including metadata
                    tracing::info!("Querying upload at {}", target_url);
                    let info = client.get_upload_info(&target_url).await?;
                    println!("{}", info);
                } else {
                    // Query server capabilities
                    tracing::info!("Querying TUS server at {}", target_url);
                    let capabilities = client.discover_capabilities(&target_url).await?;
                    println!("{}", capabilities);
                }
            } else {
                // Show local configuration
                println!("ztus Configuration");
                println!("===================");
                println!();
                println!("State Directory: {}", client.state_dir().display());
                println!();
                println!("Upload Configuration:");
                println!("  Chunk Size: {} MB", client.upload_config().chunk_size / 1024 / 1024);
                println!("  TUS Version: {}", client.upload_config().tus_version);
                println!("  Max Retries: {}", client.upload_config().max_retries);
                println!("  Timeout: {} seconds", client.upload_config().timeout);
                println!("  Verify Checksum: {}", client.upload_config().verify_checksum);
            }
        }

        Commands::Terminate { upload_url } => {
            tracing::info!("Terminating upload at {}", upload_url);
            client.terminate_upload(&upload_url).await?;
            println!("Upload terminated successfully");
        }

        Commands::Config { action } => match action {
            ConfigCommands::Get { key } => {
                let config = AppConfig::load()?;
                match config.get_value(&key) {
                    Ok(value) => println!("{}", value),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            ConfigCommands::Set { key, value } => {
                let mut config = AppConfig::load()?;
                match config.set_value(&key, &value) {
                    Ok(()) => {
                        config.save()?;
                        println!("Set {} = {}", key, value);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            ConfigCommands::List => {
                let config = AppConfig::load()?;
                println!("ztus Configuration");
                println!("===================");
                println!();
                println!("Config File: {}", AppConfig::config_path().display());
                println!("State Directory: {}", config.state_dir.display());
                println!();
                println!("Upload Settings:");
                println!("  upload.chunk_size        = {} ({} MB)",
                    config.upload.chunk_size,
                    config.upload.chunk_size / 1024 / 1024
                );
                println!("  upload.tus_version       = {}", config.upload.tus_version);
                println!("  upload.max_retries       = {}", config.upload.max_retries);
                println!("  upload.timeout           = {}", config.upload.timeout);
                println!("  upload.verify_checksum   = {}", config.upload.verify_checksum);
                println!("  upload.checksum_algorithm = {:?}", config.upload.checksum_algorithm);
            }

            ConfigCommands::Edit => {
                let config_path = AppConfig::config_path();

                // Create config file if it doesn't exist
                if !config_path.exists() {
                    println!("Config file doesn't exist. Creating default config...");
                    let config = AppConfig::load()?;
                    config.save()?;
                }

                // Open in editor
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
                    // Try common editors
                    if cfg!(windows) {
                        "notepad".to_string()
                    } else {
                        "vi".to_string()
                    }
                });

                println!("Opening {} in {}...", config_path.display(), editor);

                let status = std::process::Command::new(&editor)
                    .arg(&config_path)
                    .status()
                    .map_err(|e| {
                        eprintln!("Failed to open editor '{}': {}", editor, e);
                        e
                    })?;

                if !status.success() {
                    eprintln!("Editor exited with non-zero status");
                    std::process::exit(1);
                }

                // Validate the edited config
                match AppConfig::load() {
                    Ok(_) => println!("Configuration is valid."),
                    Err(e) => {
                        eprintln!("Warning: Configuration validation failed: {}", e);
                        eprintln!("Please fix the configuration and try again.");
                        std::process::exit(1);
                    }
                }
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_metadata_single() {
        let metadata = vec!["filename:test.txt".to_string()];
        let result = parse_metadata(&metadata).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("filename").unwrap(), "test.txt");
    }

    #[test]
    fn test_parse_metadata_multiple() {
        let metadata = vec![
            "filename:test.txt".to_string(),
            "type:document".to_string(),
            "author:John Doe".to_string(),
        ];
        let result = parse_metadata(&metadata).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("filename").unwrap(), "test.txt");
        assert_eq!(result.get("type").unwrap(), "document");
        assert_eq!(result.get("author").unwrap(), "John Doe");
    }

    #[test]
    fn test_parse_metadata_with_colon_in_value() {
        let metadata = vec!["time:10:30".to_string()];
        let result = parse_metadata(&metadata).unwrap();
        assert_eq!(result.len(), 1);
        // split_once only splits on the first colon, so value should be "10:30"
        assert_eq!(result.get("time").unwrap(), "10:30");
    }

    #[test]
    fn test_parse_metadata_empty_value() {
        let metadata = vec!["filename:".to_string()];
        let result = parse_metadata(&metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_parse_metadata_empty_key() {
        let metadata = vec![":value".to_string()];
        let result = parse_metadata(&metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("key cannot be empty"));
    }

    #[test]
    fn test_parse_metadata_no_colon() {
        let metadata = vec!["filename".to_string()];
        let result = parse_metadata(&metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid metadata format"));
    }

    #[test]
    fn test_parse_metadata_empty_vec() {
        let metadata = vec![];
        let result = parse_metadata(&metadata).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_metadata_special_characters() {
        let metadata = vec![
            "filename:test file.txt".to_string(),
            "path:/path/to/file".to_string(),
            "url:https://example.com".to_string(),
        ];
        let result = parse_metadata(&metadata).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("filename").unwrap(), "test file.txt");
        assert_eq!(result.get("path").unwrap(), "/path/to/file");
        assert_eq!(result.get("url").unwrap(), "https://example.com");
    }

    #[test]
    fn test_parse_headers_single() {
        let headers = vec!["X-API-Key:secret123".to_string()];
        let result = parse_headers(&headers).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "X-API-Key");
        assert_eq!(result[0].1, "secret123");
    }

    #[test]
    fn test_parse_headers_multiple() {
        let headers = vec![
            "X-API-Key:secret123".to_string(),
            "Authorization:Bearer token".to_string(),
            "Content-Type:application/json".to_string(),
        ];
        let result = parse_headers(&headers).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], (String::from("X-API-Key"), String::from("secret123")));
        assert_eq!(result[1], (String::from("Authorization"), String::from("Bearer token")));
        assert_eq!(result[2], (String::from("Content-Type"), String::from("application/json")));
    }

    #[test]
    fn test_parse_headers_with_spaces() {
        let headers = vec![
            "X-API-Key: secret123".to_string(),
            "Authorization: Bearer token".to_string(),
        ];
        let result = parse_headers(&headers).unwrap();
        assert_eq!(result.len(), 2);
        // Spaces should be trimmed
        assert_eq!(result[0].0, "X-API-Key");
        assert_eq!(result[0].1, "secret123");
        assert_eq!(result[1].0, "Authorization");
        assert_eq!(result[1].1, "Bearer token");
    }

    #[test]
    fn test_parse_headers_with_colon_in_value() {
        let headers = vec!["Time:10:30".to_string()];
        let result = parse_headers(&headers).unwrap();
        assert_eq!(result.len(), 1);
        // split_once only splits on the first colon, so value should be "10:30"
        assert_eq!(result[0].0, "Time");
        assert_eq!(result[0].1, "10:30");
    }

    #[test]
    fn test_parse_headers_empty_value() {
        let headers = vec!["X-API-Key:".to_string()];
        let result = parse_headers(&headers);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_parse_headers_empty_key() {
        let headers = vec![":secret123".to_string()];
        let result = parse_headers(&headers);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("key cannot be empty"));
    }

    #[test]
    fn test_parse_headers_no_colon() {
        let headers = vec!["X-API-Key".to_string()];
        let result = parse_headers(&headers);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid header format"));
    }

    #[test]
    fn test_parse_headers_empty_vec() {
        let headers = vec![];
        let result = parse_headers(&headers).unwrap();
        assert_eq!(result.len(), 0);
    }
}

