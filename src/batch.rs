//! Batch upload support for TUS protocol
//!
//! This module provides functionality for uploading multiple files as a batch,
//! using the ZDownloadAPI batch API endpoints.

use crate::config::UploadConfig;
use crate::error::{Result, ZtusError};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Request for a single file in a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFileRequest {
    /// Filename of the file
    pub filename: String,
    /// Size in bytes
    pub size: u64,
}

/// Request to create a batch upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateRequest {
    /// List of files to upload
    pub files: Vec<BatchFileRequest>,
    /// Optional metadata for the batch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Information about a single upload in a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchUploadInfo {
    /// Filename of the file
    pub filename: String,
    /// Size in bytes
    pub size: u64,
    /// TUS upload URL (with batch_id query param already included)
    pub upload_url: String,
}

/// Response from creating a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateResponse {
    /// Unique batch identifier
    pub batch_id: String,
    /// List of uploads with their URLs
    pub uploads: Vec<BatchUploadInfo>,
}

/// Batch status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatusResponse {
    /// Unique batch identifier
    pub batch_id: String,
    /// Current status: "pending", "in_progress", or "completed"
    pub status: String,
    /// Total number of files in the batch
    pub total_files: u32,
    /// Number of completed file uploads
    pub completed_files: u32,
    /// Overall progress (0.0 to 1.0)
    pub progress: f64,
    /// Optional metadata stored with the batch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

impl std::fmt::Display for BatchStatusResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Batch Status")?;
        writeln!(f, "============")?;
        writeln!(f)?;
        writeln!(f, "Batch ID: {}", self.batch_id)?;
        writeln!(f, "Status: {}", self.status)?;
        writeln!(
            f,
            "Progress: {}/{} files ({:.1}%)",
            self.completed_files,
            self.total_files,
            self.progress * 100.0
        )?;
        if let Some(ref meta) = self.metadata {
            writeln!(f)?;
            writeln!(f, "Metadata: {}", meta)?;
        }
        Ok(())
    }
}

/// Batch upload client
pub struct BatchClient {
    client: Client,
    headers: Vec<(String, String)>,
}

impl BatchClient {
    /// Create a new batch client
    #[allow(dead_code)]
    pub fn new(timeout: Duration) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .tcp_nodelay(true)
            .build()
            .map_err(ZtusError::from)?;

        Ok(Self {
            client,
            headers: Vec::new(),
        })
    }

    /// Create a batch client with custom headers
    pub fn with_headers(timeout: Duration, headers: Vec<(String, String)>) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .tcp_nodelay(true)
            .build()
            .map_err(ZtusError::from)?;

        Ok(Self { client, headers })
    }

    /// Apply custom headers to a request
    fn apply_headers(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }
        request
    }

    /// Create a new batch upload
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the upload server (e.g., "http://localhost:8080")
    /// * `files` - List of files with their names and sizes
    /// * `metadata` - Optional metadata to attach to the batch
    ///
    /// # Returns
    /// A `BatchCreateResponse` containing the batch ID and upload URLs for each file
    pub async fn create_batch(
        &self,
        base_url: &str,
        files: Vec<BatchFileRequest>,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<BatchCreateResponse> {
        let batch_url = format!("{}/batch", base_url.trim_end_matches('/'));
        let request_body = BatchCreateRequest { files, metadata };

        let request = self.client.post(&batch_url).json(&request_body);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unable to read response body>".to_string());

            let error_msg = if error_body.is_empty() || error_body.contains('<') {
                format!("Failed to create batch: {}", status)
            } else {
                format!(
                    "Failed to create batch: {} - Server error: {}",
                    status,
                    error_body.trim()
                )
            };
            return Err(ZtusError::ProtocolError(error_msg));
        }

        response.json().await.map_err(|e| {
            ZtusError::ProtocolError(format!("Failed to parse batch response: {}", e))
        })
    }

    /// Get the status of a batch upload
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the upload server
    /// * `batch_id` - The batch identifier
    ///
    /// # Returns
    /// A `BatchStatusResponse` with the current batch status
    pub async fn get_batch_status(
        &self,
        base_url: &str,
        batch_id: &str,
    ) -> Result<BatchStatusResponse> {
        let status_url = format!("{}/batch/{}", base_url.trim_end_matches('/'), batch_id);

        let request = self.client.get(&status_url);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ZtusError::ProtocolError(format!(
                "Batch not found: {}",
                batch_id
            )));
        }

        if !response.status().is_success() {
            let status = response.status();
            return Err(ZtusError::ProtocolError(format!(
                "Failed to get batch status: {}",
                status
            )));
        }

        response.json().await.map_err(|e| {
            ZtusError::ProtocolError(format!("Failed to parse batch status response: {}", e))
        })
    }
}

/// Collect file information for batch upload
///
/// # Arguments
/// * `files` - List of file paths to include in the batch
///
/// # Returns
/// A vector of `BatchFileRequest` with filename and size for each file
pub fn collect_file_info(files: &[PathBuf]) -> Result<Vec<BatchFileRequest>> {
    files
        .iter()
        .map(|f| {
            let metadata = std::fs::metadata(f).map_err(|e| {
                ZtusError::FileNotFound(format!("{}: {}", f.display(), e))
            })?;

            let filename = f
                .file_name()
                .ok_or_else(|| ZtusError::ConfigError("File has no name".to_string()))?
                .to_string_lossy()
                .to_string();

            Ok(BatchFileRequest {
                filename,
                size: metadata.len(),
            })
        })
        .collect()
}

/// Result of a batch upload operation
#[derive(Debug)]
pub struct BatchUploadResult {
    /// The batch information from the server
    pub batch: BatchCreateResponse,
    /// Number of successfully uploaded files
    pub successful: usize,
    /// Number of failed uploads
    pub failed: usize,
    /// Error messages for failed uploads (keyed by filename)
    pub errors: HashMap<String, String>,
}

/// Execute a batch upload
///
/// Creates a batch on the server and uploads each file using the TUS protocol.
///
/// # Arguments
/// * `base_url` - Base URL of the upload server
/// * `files` - List of file paths to upload
/// * `config` - Upload configuration
/// * `headers` - Custom HTTP headers
///
/// # Returns
/// A `BatchUploadResult` with the outcome of the upload
pub async fn execute_batch_upload(
    base_url: &str,
    files: Vec<PathBuf>,
    config: &UploadConfig,
    headers: Vec<(String, String)>,
) -> Result<BatchUploadResult> {
    use crate::upload::UploadManager;

    // Collect file information
    let file_requests = collect_file_info(&files)?;

    // Create batch client with headers
    let batch_client =
        BatchClient::with_headers(Duration::from_secs(config.timeout), headers.clone())?;

    // Create the batch
    let batch = batch_client
        .create_batch(base_url, file_requests, None)
        .await?;

    tracing::info!("Created batch {} with {} files", batch.batch_id, batch.uploads.len());

    // Upload each file
    let mut successful = 0;
    let mut failed = 0;
    let mut errors: HashMap<String, String> = HashMap::new();

    // Create a state directory for upload state
    let state_dir = dirs::home_dir()
        .map(|p| p.join(".ztus").join("state"))
        .unwrap_or_else(|| PathBuf::from(".ztus/state"));

    std::fs::create_dir_all(&state_dir).map_err(ZtusError::from)?;

    for (file, upload_info) in files.iter().zip(batch.uploads.iter()) {
        tracing::info!(
            "Uploading {} ({} bytes) to {}",
            upload_info.filename,
            upload_info.size,
            upload_info.upload_url
        );

        // Create upload manager for this file
        let mut upload_config = config.clone();
        upload_config.headers = headers.clone();

        let manager = UploadManager::new(
            upload_info.upload_url.clone(),
            upload_config,
            state_dir.clone(),
        )?;

        match manager.upload_file(file).await {
            Ok(()) => {
                successful += 1;
                tracing::info!("Successfully uploaded {}", upload_info.filename);
            }
            Err(e) => {
                failed += 1;
                let error_msg = format!("{}", e);
                tracing::error!("Failed to upload {}: {}", upload_info.filename, error_msg);
                errors.insert(upload_info.filename.clone(), error_msg);
            }
        }
    }

    Ok(BatchUploadResult {
        batch,
        successful,
        failed,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_batch_file_request_serialization() {
        let request = BatchFileRequest {
            filename: "test.txt".to_string(),
            size: 1024,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"filename\":\"test.txt\""));
        assert!(json.contains("\"size\":1024"));
    }

    #[test]
    fn test_batch_create_request_serialization() {
        let request = BatchCreateRequest {
            files: vec![
                BatchFileRequest {
                    filename: "file1.txt".to_string(),
                    size: 100,
                },
                BatchFileRequest {
                    filename: "file2.txt".to_string(),
                    size: 200,
                },
            ],
            metadata: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"files\""));
        assert!(json.contains("\"file1.txt\""));
        assert!(json.contains("\"file2.txt\""));
        assert!(!json.contains("metadata"));
    }

    #[test]
    fn test_batch_create_request_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("user_id".to_string(), "12345".to_string());

        let request = BatchCreateRequest {
            files: vec![BatchFileRequest {
                filename: "test.txt".to_string(),
                size: 100,
            }],
            metadata: Some(metadata),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"metadata\""));
        assert!(json.contains("\"user_id\""));
        assert!(json.contains("\"12345\""));
    }

    #[test]
    fn test_batch_status_response_deserialization() {
        let json = r#"{
            "batch_id": "abc123",
            "status": "in_progress",
            "total_files": 5,
            "completed_files": 2,
            "progress": 0.4
        }"#;

        let response: BatchStatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.batch_id, "abc123");
        assert_eq!(response.status, "in_progress");
        assert_eq!(response.total_files, 5);
        assert_eq!(response.completed_files, 2);
        assert!((response.progress - 0.4).abs() < 0.001);
        assert!(response.metadata.is_none());
    }

    #[test]
    fn test_batch_status_display() {
        let status = BatchStatusResponse {
            batch_id: "test-batch-123".to_string(),
            status: "in_progress".to_string(),
            total_files: 10,
            completed_files: 5,
            progress: 0.5,
            metadata: None,
        };

        let display = format!("{}", status);
        assert!(display.contains("test-batch-123"));
        assert!(display.contains("in_progress"));
        assert!(display.contains("5/10 files"));
        assert!(display.contains("50.0%"));
    }

    #[test]
    fn test_collect_file_info() {
        let dir = tempdir().unwrap();

        // Create test files
        let file1_path = dir.path().join("test1.txt");
        let file2_path = dir.path().join("test2.txt");

        let mut file1 = std::fs::File::create(&file1_path).unwrap();
        file1.write_all(b"Hello, World!").unwrap();

        let mut file2 = std::fs::File::create(&file2_path).unwrap();
        file2.write_all(b"Goodbye!").unwrap();

        let files = vec![file1_path.clone(), file2_path.clone()];
        let result = collect_file_info(&files).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].filename, "test1.txt");
        assert_eq!(result[0].size, 13); // "Hello, World!" = 13 bytes
        assert_eq!(result[1].filename, "test2.txt");
        assert_eq!(result[1].size, 8); // "Goodbye!" = 8 bytes
    }

    #[test]
    fn test_collect_file_info_missing_file() {
        let files = vec![PathBuf::from("/nonexistent/file.txt")];
        let result = collect_file_info(&files);
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_client_creation() {
        let client = BatchClient::new(Duration::from_secs(30));
        assert!(client.is_ok());
    }

    #[test]
    fn test_batch_client_with_headers() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer token123".to_string()),
            ("X-Custom-Header".to_string(), "value".to_string()),
        ];
        let client = BatchClient::with_headers(Duration::from_secs(30), headers);
        assert!(client.is_ok());
    }
}
