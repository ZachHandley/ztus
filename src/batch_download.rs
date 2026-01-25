//! Batch download support for ztus
//!
//! This module provides functionality for downloading multiple files as a batch,
//! using the ZDownloadAPI batch download endpoints.

use crate::download::DownloadManager;
use crate::error::{Result, ZtusError};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// File filter for batch download queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileFilter {
    /// Filename prefix match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,

    /// Filename suffix/extension match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,

    /// Filename contains substring
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,

    /// Exact filename matches (OR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,

    /// Exact hash matches (OR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<Vec<String>>,

    /// Minimum file size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_size: Option<i64>,

    /// Maximum file size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<i64>,

    /// Only include files uploaded after this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploaded_after: Option<chrono::DateTime<chrono::Utc>>,

    /// Only include files uploaded before this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploaded_before: Option<chrono::DateTime<chrono::Utc>>,

    /// Only latest version per filename
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_only: Option<bool>,

    /// Max files to include
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,

    /// Skip first N matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i32>,

    /// Order by field: "name", "size", "uploaded_at"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,

    /// Descending order
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_desc: Option<bool>,
}

impl FileFilter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Set prefix filter
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Set suffix filter
    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    /// Set contains filter
    pub fn with_contains(mut self, contains: impl Into<String>) -> Self {
        self.contains = Some(contains.into());
        self
    }

    /// Set names filter
    #[allow(dead_code)]
    pub fn with_names(mut self, names: Vec<String>) -> Self {
        self.names = Some(names);
        self
    }

    /// Set hashes filter
    #[allow(dead_code)]
    pub fn with_hashes(mut self, hashes: Vec<String>) -> Self {
        self.hashes = Some(hashes);
        self
    }

    /// Set min size filter
    pub fn with_min_size(mut self, min_size: i64) -> Self {
        self.min_size = Some(min_size);
        self
    }

    /// Set max size filter
    pub fn with_max_size(mut self, max_size: i64) -> Self {
        self.max_size = Some(max_size);
        self
    }

    /// Set limit
    pub fn with_limit(mut self, limit: i32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set latest only filter
    pub fn with_latest_only(mut self, latest_only: bool) -> Self {
        self.latest_only = Some(latest_only);
        self
    }

    /// Set order by field
    #[allow(dead_code)]
    pub fn with_order_by(mut self, order_by: impl Into<String>) -> Self {
        self.order_by = Some(order_by.into());
        self
    }

    /// Set descending order
    #[allow(dead_code)]
    pub fn with_order_desc(mut self, order_desc: bool) -> Self {
        self.order_desc = Some(order_desc);
        self
    }
}

/// Request to create a batch download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDownloadCreateRequest {
    /// Filter criteria for selecting files
    pub filter: FileFilter,

    /// Output format: "urls" or "zip" (default: "urls")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,

    /// Optional metadata for the batch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,

    /// Hours until batch expires (default: 1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_hours: Option<i32>,
}

/// Response from creating a batch download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDownloadCreateResponse {
    /// Unique batch identifier
    pub batch_id: String,
    /// Current status of the batch
    pub status: String,
    /// Total number of files in the batch
    pub total_files: i32,
    /// Total size in bytes
    pub total_size: i64,
    /// Output format
    pub output_format: String,
    /// When the batch expires
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the batch was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// URL to check batch status
    pub status_url: String,
}

/// Batch download status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDownloadStatusResponse {
    /// Unique batch identifier
    pub batch_id: String,
    /// Current status: pending, preparing, ready, in_progress, completed, failed, expired
    pub status: String,
    /// Total number of files in the batch
    pub total_files: i32,
    /// Total size in bytes
    pub total_size: i64,
    /// Number of files prepared for download
    pub prepared_files: i32,
    /// Number of files actually downloaded
    pub downloaded_files: i32,
    /// Overall progress (0.0 to 1.0)
    pub progress: f64,
    /// Output format
    pub output_format: String,
    /// When the batch expires
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the batch was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the batch completed (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl std::fmt::Display for BatchDownloadStatusResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Batch Download Status")?;
        writeln!(f, "=====================")?;
        writeln!(f)?;
        writeln!(f, "Batch ID: {}", self.batch_id)?;
        writeln!(f, "Status: {}", self.status)?;
        writeln!(
            f,
            "Progress: {}/{} files ({:.1}%)",
            self.downloaded_files,
            self.total_files,
            self.progress * 100.0
        )?;
        writeln!(
            f,
            "Total Size: {} bytes ({:.2} MB)",
            self.total_size,
            self.total_size as f64 / 1024.0 / 1024.0
        )?;
        writeln!(f, "Output Format: {}", self.output_format)?;
        if let Some(ref expires_at) = self.expires_at {
            writeln!(f, "Expires At: {}", expires_at)?;
        }
        if let Some(ref completed_at) = self.completed_at {
            writeln!(f, "Completed At: {}", completed_at)?;
        }
        Ok(())
    }
}

/// Information about a file in a download batch
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFileInfo {
    /// File ID in the batch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// Filename
    pub filename: String,
    /// File size in bytes
    pub size: i64,
    /// File hash (SHA-256)
    pub hash: String,
    /// File status (pending, ready, downloaded, failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Direct download URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
}

/// Response containing download URLs for batch files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDownloadURLsResponse {
    /// Batch identifier
    pub batch_id: String,
    /// Total number of files
    pub total_files: i32,
    /// Total size in bytes
    pub total_size: i64,
    /// When the batch expires
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Files with download URLs
    pub files: Vec<DownloadURLInfo>,
}

/// Download URL information for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadURLInfo {
    /// Filename
    pub filename: String,
    /// File size in bytes
    pub size: i64,
    /// File hash
    pub hash: String,
    /// Direct download URL
    pub download_url: String,
}

/// Response for listing files in a batch
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFilesResponse {
    /// Batch identifier
    pub batch_id: String,
    /// Files in the batch
    pub files: Vec<DownloadFileInfo>,
    /// Total count of files matching filter
    pub total: i32,
    /// Limit used in query
    pub limit: i32,
    /// Offset used in query
    pub offset: i32,
}

/// Preview file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFileInfo {
    /// Filename
    pub filename: String,
    /// File size in bytes
    pub size: i64,
    /// File hash
    pub hash: String,
    /// When the file was uploaded
    pub uploaded_at: chrono::DateTime<chrono::Utc>,
}

/// Response from previewing a filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFilterResponse {
    /// Total number of files matching the filter
    pub total_files: i32,
    /// Total size in bytes
    pub total_size: i64,
    /// Sample of matching files (up to 10)
    pub sample_files: Vec<PreviewFileInfo>,
}

impl std::fmt::Display for PreviewFilterResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Filter Preview")?;
        writeln!(f, "==============")?;
        writeln!(f)?;
        writeln!(f, "Total Files: {}", self.total_files)?;
        writeln!(
            f,
            "Total Size: {} bytes ({:.2} MB)",
            self.total_size,
            self.total_size as f64 / 1024.0 / 1024.0
        )?;
        if !self.sample_files.is_empty() {
            writeln!(f)?;
            writeln!(f, "Sample Files:")?;
            for file in &self.sample_files {
                writeln!(
                    f,
                    "  - {} ({} bytes)",
                    file.filename, file.size
                )?;
            }
        }
        Ok(())
    }
}

/// Batch download client
pub struct BatchDownloadClient {
    client: Client,
    headers: Vec<(String, String)>,
}

impl BatchDownloadClient {
    /// Create a new batch download client
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

    /// Create a batch download client with custom headers
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

    /// Create a new batch download
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the download server (e.g., "http://localhost:8080")
    /// * `request` - Batch download create request with filter and options
    ///
    /// # Returns
    /// A `BatchDownloadCreateResponse` containing the batch ID and status URL
    pub async fn create_batch(
        &self,
        base_url: &str,
        request: BatchDownloadCreateRequest,
    ) -> Result<BatchDownloadCreateResponse> {
        let batch_url = format!("{}/download/batch", base_url.trim_end_matches('/'));

        let req = self.client.post(&batch_url).json(&request);
        let req = self.apply_headers(req);

        let response = req.send().await.map_err(ZtusError::from)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unable to read response body>".to_string());

            let error_msg = if error_body.is_empty() || error_body.contains('<') {
                format!("Failed to create batch download: {}", status)
            } else {
                format!(
                    "Failed to create batch download: {} - Server error: {}",
                    status,
                    error_body.trim()
                )
            };
            return Err(ZtusError::ProtocolError(error_msg));
        }

        response.json().await.map_err(|e| {
            ZtusError::ProtocolError(format!("Failed to parse batch download response: {}", e))
        })
    }

    /// Get the status of a batch download
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the download server
    /// * `batch_id` - The batch identifier
    ///
    /// # Returns
    /// A `BatchDownloadStatusResponse` with the current batch status
    pub async fn get_status(
        &self,
        base_url: &str,
        batch_id: &str,
    ) -> Result<BatchDownloadStatusResponse> {
        let status_url = format!(
            "{}/download/batch/{}",
            base_url.trim_end_matches('/'),
            batch_id
        );

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

    /// Get download URLs for a batch
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the download server
    /// * `batch_id` - The batch identifier
    ///
    /// # Returns
    /// A `BatchDownloadURLsResponse` with download URLs for each file
    pub async fn get_download_urls(
        &self,
        base_url: &str,
        batch_id: &str,
    ) -> Result<BatchDownloadURLsResponse> {
        let download_url = format!(
            "{}/download/batch/{}/download?format=urls",
            base_url.trim_end_matches('/'),
            batch_id
        );

        let request = self.client.get(&download_url);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(ZtusError::ProtocolError(format!(
                "Batch not found: {}",
                batch_id
            )));
        }

        if response.status() == StatusCode::CONFLICT {
            return Err(ZtusError::ProtocolError(
                "Batch is not ready for download".to_string(),
            ));
        }

        if response.status() == StatusCode::GONE {
            return Err(ZtusError::ProtocolError("Batch has expired".to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unable to read response body>".to_string());

            return Err(ZtusError::ProtocolError(format!(
                "Failed to get download URLs: {} - {}",
                status,
                error_body.trim()
            )));
        }

        response.json().await.map_err(|e| {
            ZtusError::ProtocolError(format!("Failed to parse download URLs response: {}", e))
        })
    }

    /// List files in a batch
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the download server
    /// * `batch_id` - The batch identifier
    /// * `status` - Optional status filter (pending, ready, downloaded, failed)
    /// * `limit` - Max results (default: 100)
    /// * `offset` - Pagination offset (default: 0)
    ///
    /// # Returns
    /// A `BatchFilesResponse` with the files in the batch
    #[allow(dead_code)]
    pub async fn list_files(
        &self,
        base_url: &str,
        batch_id: &str,
        status: Option<&str>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<BatchFilesResponse> {
        let mut files_url = format!(
            "{}/download/batch/{}/files",
            base_url.trim_end_matches('/'),
            batch_id
        );

        let mut query_params = Vec::new();
        if let Some(s) = status {
            query_params.push(format!("status={}", s));
        }
        if let Some(l) = limit {
            query_params.push(format!("limit={}", l));
        }
        if let Some(o) = offset {
            query_params.push(format!("offset={}", o));
        }
        if !query_params.is_empty() {
            files_url.push('?');
            files_url.push_str(&query_params.join("&"));
        }

        let request = self.client.get(&files_url);
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
                "Failed to list batch files: {}",
                status
            )));
        }

        response.json().await.map_err(|e| {
            ZtusError::ProtocolError(format!("Failed to parse batch files response: {}", e))
        })
    }

    /// Delete a batch download
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the download server
    /// * `batch_id` - The batch identifier
    pub async fn delete_batch(&self, base_url: &str, batch_id: &str) -> Result<()> {
        let delete_url = format!(
            "{}/download/batch/{}",
            base_url.trim_end_matches('/'),
            batch_id
        );

        let request = self.client.delete(&delete_url);
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
                "Failed to delete batch: {}",
                status
            )));
        }

        Ok(())
    }

    /// Preview files matching a filter without creating a batch
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the download server
    /// * `filter` - File filter to preview
    ///
    /// # Returns
    /// A `PreviewFilterResponse` with file count, total size, and sample files
    pub async fn preview_filter(
        &self,
        base_url: &str,
        filter: FileFilter,
    ) -> Result<PreviewFilterResponse> {
        let preview_url = format!("{}/download/preview", base_url.trim_end_matches('/'));

        #[derive(Serialize)]
        struct PreviewRequest {
            filter: FileFilter,
        }

        let request = self
            .client
            .post(&preview_url)
            .json(&PreviewRequest { filter });
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unable to read response body>".to_string());

            return Err(ZtusError::ProtocolError(format!(
                "Failed to preview filter: {} - {}",
                status,
                error_body.trim()
            )));
        }

        response.json().await.map_err(|e| {
            ZtusError::ProtocolError(format!("Failed to parse preview response: {}", e))
        })
    }

    /// List all batches
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the download server
    /// * `status` - Optional status filter
    /// * `limit` - Max results (default: 100)
    /// * `offset` - Pagination offset (default: 0)
    ///
    /// # Returns
    /// A vector of `BatchDownloadStatusResponse`
    pub async fn list_batches(
        &self,
        base_url: &str,
        status: Option<&str>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<BatchDownloadStatusResponse>> {
        let mut batches_url = format!("{}/download/batches", base_url.trim_end_matches('/'));

        let mut query_params = Vec::new();
        if let Some(s) = status {
            query_params.push(format!("status={}", s));
        }
        if let Some(l) = limit {
            query_params.push(format!("limit={}", l));
        }
        if let Some(o) = offset {
            query_params.push(format!("offset={}", o));
        }
        if !query_params.is_empty() {
            batches_url.push('?');
            batches_url.push_str(&query_params.join("&"));
        }

        let request = self.client.get(&batches_url);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(ZtusError::ProtocolError(format!(
                "Failed to list batches: {}",
                status
            )));
        }

        response.json().await.map_err(|e| {
            ZtusError::ProtocolError(format!("Failed to parse batches response: {}", e))
        })
    }
}

/// Result of a batch download operation
#[derive(Debug)]
pub struct BatchDownloadResult {
    /// The batch information from the server
    pub batch_id: String,
    /// Number of successfully downloaded files
    pub successful: usize,
    /// Number of failed downloads
    pub failed: usize,
    /// Total bytes downloaded
    pub bytes_downloaded: u64,
    /// Error messages for failed downloads (keyed by filename)
    pub errors: HashMap<String, String>,
    /// Successfully downloaded files (paths to downloaded files)
    #[allow(dead_code)]
    pub downloaded_files: Vec<PathBuf>,
}

/// Configuration for batch download execution
#[derive(Debug, Clone)]
pub struct BatchDownloadConfig {
    /// Timeout in seconds for HTTP requests
    pub timeout: u64,
    /// Chunk size for downloads in bytes
    pub chunk_size: usize,
    /// Poll interval in milliseconds when waiting for batch to be ready
    pub poll_interval_ms: u64,
    /// Maximum time to wait for batch to be ready in seconds
    pub max_wait_time: u64,
}

impl Default for BatchDownloadConfig {
    fn default() -> Self {
        Self {
            timeout: 300,
            chunk_size: 5 * 1024 * 1024, // 5MB
            poll_interval_ms: 1000,       // 1 second
            max_wait_time: 600,           // 10 minutes
        }
    }
}

/// Execute a batch download
///
/// Creates a batch on the server, waits for it to be ready, and downloads all files.
///
/// # Arguments
/// * `base_url` - Base URL of the download server
/// * `filter` - Filter criteria for selecting files
/// * `output_dir` - Directory to save downloaded files
/// * `config` - Download configuration
/// * `headers` - Custom HTTP headers
///
/// # Returns
/// A `BatchDownloadResult` with the outcome of the download
pub async fn execute_batch_download(
    base_url: &str,
    filter: FileFilter,
    output_dir: &Path,
    config: &BatchDownloadConfig,
    headers: Vec<(String, String)>,
) -> Result<BatchDownloadResult> {
    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).map_err(ZtusError::from)?;
    }

    // Create batch client with headers
    let batch_client =
        BatchDownloadClient::with_headers(Duration::from_secs(config.timeout), headers.clone())?;

    // Create the batch
    let create_request = BatchDownloadCreateRequest {
        filter,
        output_format: Some("urls".to_string()),
        metadata: None,
        expires_in_hours: Some(1),
    };

    let batch_response = batch_client.create_batch(base_url, create_request).await?;
    let batch_id = batch_response.batch_id.clone();

    tracing::info!(
        "Created batch {} with {} files ({} bytes)",
        batch_id,
        batch_response.total_files,
        batch_response.total_size
    );

    // Wait for batch to be ready
    let start_time = std::time::Instant::now();
    loop {
        let status = batch_client.get_status(base_url, &batch_id).await?;

        match status.status.as_str() {
            "ready" | "in_progress" => {
                tracing::info!("Batch {} is ready for download", batch_id);
                break;
            }
            "completed" => {
                tracing::info!("Batch {} is already completed", batch_id);
                break;
            }
            "failed" => {
                return Err(ZtusError::ProtocolError(format!(
                    "Batch {} failed on server",
                    batch_id
                )));
            }
            "expired" => {
                return Err(ZtusError::ProtocolError(format!(
                    "Batch {} has expired",
                    batch_id
                )));
            }
            _ => {
                // pending, preparing
                if start_time.elapsed().as_secs() > config.max_wait_time {
                    return Err(ZtusError::ProtocolError(format!(
                        "Timeout waiting for batch {} to be ready (status: {})",
                        batch_id, status.status
                    )));
                }
                tracing::debug!(
                    "Batch {} status: {}, waiting...",
                    batch_id,
                    status.status
                );
                tokio::time::sleep(Duration::from_millis(config.poll_interval_ms)).await;
            }
        }
    }

    // Get download URLs
    let urls_response = batch_client.get_download_urls(base_url, &batch_id).await?;

    tracing::info!(
        "Got download URLs for {} files",
        urls_response.files.len()
    );

    // Download each file
    let mut successful = 0;
    let mut failed = 0;
    let mut bytes_downloaded: u64 = 0;
    let mut errors: HashMap<String, String> = HashMap::new();
    let mut downloaded_files: Vec<PathBuf> = Vec::new();

    // Create download manager
    let download_manager = DownloadManager::new(config.chunk_size)?;

    for file_info in urls_response.files {
        let output_path = output_dir.join(&file_info.filename);

        tracing::info!(
            "Downloading {} ({} bytes) to {}",
            file_info.filename,
            file_info.size,
            output_path.display()
        );

        match download_manager
            .download_file(&file_info.download_url, &output_path)
            .await
        {
            Ok(()) => {
                successful += 1;
                bytes_downloaded += file_info.size as u64;
                downloaded_files.push(output_path);
                tracing::info!("Successfully downloaded {}", file_info.filename);
            }
            Err(e) => {
                failed += 1;
                let error_msg = format!("{}", e);
                tracing::error!("Failed to download {}: {}", file_info.filename, error_msg);
                errors.insert(file_info.filename.clone(), error_msg);
            }
        }
    }

    Ok(BatchDownloadResult {
        batch_id,
        successful,
        failed,
        bytes_downloaded,
        errors,
        downloaded_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_filter_builder() {
        let filter = FileFilter::new()
            .with_prefix("test_")
            .with_suffix(".txt")
            .with_max_size(1024 * 1024)
            .with_limit(10);

        assert_eq!(filter.prefix, Some("test_".to_string()));
        assert_eq!(filter.suffix, Some(".txt".to_string()));
        assert_eq!(filter.max_size, Some(1024 * 1024));
        assert_eq!(filter.limit, Some(10));
    }

    #[test]
    fn test_file_filter_serialization() {
        let filter = FileFilter::new()
            .with_prefix("test_")
            .with_min_size(100)
            .with_max_size(1000);

        let json = serde_json::to_string(&filter).unwrap();
        assert!(json.contains("\"prefix\":\"test_\""));
        assert!(json.contains("\"min_size\":100"));
        assert!(json.contains("\"max_size\":1000"));
        // Empty optional fields should not be serialized
        assert!(!json.contains("suffix"));
    }

    #[test]
    fn test_batch_download_create_request_serialization() {
        let request = BatchDownloadCreateRequest {
            filter: FileFilter::new().with_prefix("data_"),
            output_format: Some("urls".to_string()),
            metadata: None,
            expires_in_hours: Some(2),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"filter\""));
        assert!(json.contains("\"output_format\":\"urls\""));
        assert!(json.contains("\"expires_in_hours\":2"));
        assert!(!json.contains("\"metadata\""));
    }

    #[test]
    fn test_batch_status_display() {
        let status = BatchDownloadStatusResponse {
            batch_id: "test-batch-123".to_string(),
            status: "in_progress".to_string(),
            total_files: 10,
            total_size: 10 * 1024 * 1024, // 10 MB
            prepared_files: 10,
            downloaded_files: 5,
            progress: 0.5,
            output_format: "urls".to_string(),
            expires_at: None,
            created_at: chrono::Utc::now(),
            completed_at: None,
        };

        let display = format!("{}", status);
        assert!(display.contains("test-batch-123"));
        assert!(display.contains("in_progress"));
        assert!(display.contains("5/10 files"));
        assert!(display.contains("50.0%"));
    }

    #[test]
    fn test_preview_response_display() {
        let preview = PreviewFilterResponse {
            total_files: 25,
            total_size: 50 * 1024 * 1024, // 50 MB
            sample_files: vec![
                PreviewFileInfo {
                    filename: "file1.txt".to_string(),
                    size: 1024,
                    hash: "abc123".to_string(),
                    uploaded_at: chrono::Utc::now(),
                },
            ],
        };

        let display = format!("{}", preview);
        assert!(display.contains("Total Files: 25"));
        assert!(display.contains("file1.txt"));
    }

    #[test]
    fn test_batch_download_config_default() {
        let config = BatchDownloadConfig::default();
        assert_eq!(config.timeout, 300);
        assert_eq!(config.chunk_size, 5 * 1024 * 1024);
        assert_eq!(config.poll_interval_ms, 1000);
        assert_eq!(config.max_wait_time, 600);
    }

    #[test]
    fn test_batch_download_client_creation() {
        let client = BatchDownloadClient::new(Duration::from_secs(30));
        assert!(client.is_ok());
    }

    #[test]
    fn test_batch_download_client_with_headers() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer token123".to_string()),
            ("X-Custom-Header".to_string(), "value".to_string()),
        ];
        let client = BatchDownloadClient::with_headers(Duration::from_secs(30), headers);
        assert!(client.is_ok());
    }
}
