//! HTTP API server for ztus
//!
//! This module provides a REST API server that wraps ztus functionality,
//! allowing any language with HTTP support to use the TUS client.

use crate::client::TusClient;
use crate::config::UploadConfig;
use crate::error::{Result as ZtusResult, ZtusError};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// Upload request payload
#[derive(Debug, Deserialize)]
pub struct UploadRequest {
    /// Path to the file to upload
    pub file_path: String,

    /// TUS upload endpoint URL
    pub upload_url: String,

    /// Optional upload configuration
    #[serde(default)]
    pub config: Option<UploadConfigRequest>,
}

/// Upload configuration for API requests
#[derive(Debug, Deserialize, Clone)]
pub struct UploadConfigRequest {
    /// Chunk size in bytes
    #[serde(default)]
    pub chunk_size: Option<usize>,

    /// Maximum number of retry attempts
    #[serde(default)]
    pub max_retries: Option<usize>,

    /// Request timeout in seconds
    #[serde(default)]
    pub timeout: Option<u64>,

    /// Whether to resume existing uploads
    #[serde(default)]
    pub resume: Option<bool>,

    /// Whether to verify checksums
    #[serde(default)]
    pub verify_checksum: Option<bool>,

    /// Checksum algorithm: "sha1" or "sha256"
    #[serde(default)]
    pub checksum_algorithm: Option<String>,

    /// Custom HTTP headers
    #[serde(default)]
    pub headers: Option<Vec<(String, String)>>,

    /// Upload metadata
    #[serde(default)]
    pub metadata: Option<Vec<(String, String)>>,

    /// Disable adaptive chunk sizing
    #[serde(default)]
    pub disable_adaptive: Option<bool>,

    /// Initial chunk size for adaptive mode
    #[serde(default)]
    pub adaptive_initial_chunk_size: Option<usize>,

    /// Minimum chunk size for adaptive mode
    #[serde(default)]
    pub adaptive_min_chunk_size: Option<usize>,

    /// Maximum chunk size for adaptive mode
    #[serde(default)]
    pub adaptive_max_chunk_size: Option<usize>,
}

impl From<UploadConfigRequest> for UploadConfig {
    fn from(req: UploadConfigRequest) -> Self {
        let mut config = UploadConfig::default();

        if let Some(chunk_size) = req.chunk_size {
            config.chunk_size = chunk_size;
            // Explicit chunk size disables adaptive mode
            config.adaptive.enabled = false;
        }

        if let Some(max_retries) = req.max_retries {
            config.max_retries = max_retries;
        }

        if let Some(timeout) = req.timeout {
            config.timeout = timeout;
        }

        if let Some(resume) = req.resume {
            config.resume = resume;
        }

        if let Some(verify_checksum) = req.verify_checksum {
            config.verify_checksum = verify_checksum;
        }

        if let Some(checksum_algorithm) = req.checksum_algorithm {
            config.checksum_algorithm = match checksum_algorithm.to_lowercase().as_str() {
                "sha1" => crate::config::ChecksumAlgorithm::Sha1,
                "sha256" => crate::config::ChecksumAlgorithm::Sha256,
                _ => crate::config::ChecksumAlgorithm::Sha256,
            };
        }

        if let Some(headers) = req.headers {
            config.headers = headers;
        }

        if let Some(metadata) = req.metadata {
            config.metadata = metadata;
        }

        if let Some(disable_adaptive) = req.disable_adaptive {
            config.adaptive.enabled = !disable_adaptive;
        }

        if let Some(initial_chunk_size) = req.adaptive_initial_chunk_size {
            config.adaptive.initial_chunk_size = initial_chunk_size;
        }

        if let Some(min_chunk_size) = req.adaptive_min_chunk_size {
            config.adaptive.min_chunk_size = min_chunk_size;
        }

        if let Some(max_chunk_size) = req.adaptive_max_chunk_size {
            config.adaptive.max_chunk_size = max_chunk_size;
        }

        config
    }
}

/// Upload response
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    /// Success status
    pub success: bool,

    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Download request payload
#[derive(Debug, Deserialize)]
pub struct DownloadRequest {
    /// URL to download from
    pub url: String,

    /// Output file path
    pub output_path: String,

    /// Optional chunk size in bytes
    #[serde(default)]
    pub chunk_size: Option<usize>,
}

/// Download response
#[derive(Debug, Serialize)]
pub struct DownloadResponse {
    /// Success status
    pub success: bool,

    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// List uploads response
#[derive(Debug, Serialize)]
pub struct ListUploadsResponse {
    /// List of incomplete upload identifiers
    pub uploads: Vec<String>,

    /// Count of uploads
    pub count: usize,
}

/// Cleanup request payload
#[derive(Debug, Deserialize)]
pub struct CleanupRequest {
    /// Number of days of incomplete uploads to keep
    #[serde(default = "default_cleanup_days")]
    pub days: i64,
}

fn default_cleanup_days() -> i64 {
    7
}

/// Cleanup response
#[derive(Debug, Serialize)]
pub struct CleanupResponse {
    /// Number of uploads cleaned up
    pub cleaned: usize,
}

/// Terminate upload request
#[derive(Debug, Deserialize)]
pub struct TerminateRequest {
    /// Upload URL to terminate
    pub upload_url: String,
}

/// Terminate response
#[derive(Debug, Serialize)]
pub struct TerminateResponse {
    /// Success status
    pub success: bool,

    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Server capabilities response
#[derive(Debug, Serialize)]
pub struct ServerInfoResponse {
    /// Server version
    pub version: &'static str,

    /// Available endpoints
    pub endpoints: Vec<EndpointInfo>,
}

/// Endpoint information
#[derive(Debug, Serialize)]
pub struct EndpointInfo {
    /// Endpoint path
    pub path: String,

    /// HTTP method
    pub method: String,

    /// Description
    pub description: String,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,

    /// Optional error code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = if self.code.as_deref() == Some("not_found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        (status, Json(self)).into_response()
    }
}

impl From<ZtusError> for ErrorResponse {
    fn from(err: ZtusError) -> Self {
        let code = match &err {
            ZtusError::IoError(_) => Some("io_error".to_string()),
            ZtusError::HttpError(_) => Some("http_error".to_string()),
            ZtusError::ProtocolError(_) => Some("protocol_error".to_string()),
            ZtusError::ChecksumMismatch { .. } => Some("checksum_mismatch".to_string()),
            ZtusError::InvalidState(_) => Some("invalid_state".to_string()),
            ZtusError::ConfigError(_) => Some("config_error".to_string()),
            _ => None,
        };

        Self {
            error: err.to_string(),
            code,
        }
    }
}

/// Health check handler
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "ztus",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Server info handler
async fn server_info() -> Json<ServerInfoResponse> {
    Json(ServerInfoResponse {
        version: env!("CARGO_PKG_VERSION"),
        endpoints: vec![
            EndpointInfo {
                path: "/health".to_string(),
                method: "GET".to_string(),
                description: "Health check endpoint".to_string(),
            },
            EndpointInfo {
                path: "/info".to_string(),
                method: "GET".to_string(),
                description: "Server information and available endpoints".to_string(),
            },
            EndpointInfo {
                path: "/upload".to_string(),
                method: "POST".to_string(),
                description: "Upload a file to a TUS endpoint".to_string(),
            },
            EndpointInfo {
                path: "/download".to_string(),
                method: "POST".to_string(),
                description: "Download a file from a URL".to_string(),
            },
            EndpointInfo {
                path: "/uploads".to_string(),
                method: "GET".to_string(),
                description: "List incomplete uploads".to_string(),
            },
            EndpointInfo {
                path: "/cleanup".to_string(),
                method: "POST".to_string(),
                description: "Clean up old incomplete uploads".to_string(),
            },
            EndpointInfo {
                path: "/terminate".to_string(),
                method: "POST".to_string(),
                description: "Terminate an upload".to_string(),
            },
        ],
    })
}

/// Upload handler
async fn upload_handler(
    Json(payload): Json<UploadRequest>,
) -> Result<Json<UploadResponse>, ErrorResponse> {
    let client = TusClient::new().map_err(|e| ErrorResponse::from(e))?;

    let file_path = PathBuf::from(&payload.file_path);
    let config = if let Some(req_config) = payload.config {
        let upload_config: UploadConfig = req_config.into();
        upload_config.validate().map_err(|e| ErrorResponse::from(e))?;
        upload_config
    } else {
        client.upload_config().clone()
    };

    client
        .upload_with_config(&file_path, &payload.upload_url, &config)
        .await
        .map_err(|e| ErrorResponse::from(e))?;

    Ok(Json(UploadResponse {
        success: true,
        message: Some("Upload completed successfully".to_string()),
    }))
}

/// Download handler
async fn download_handler(
    Json(payload): Json<DownloadRequest>,
) -> Result<Json<DownloadResponse>, ErrorResponse> {
    let client = TusClient::new().map_err(|e| ErrorResponse::from(e))?;

    let output_path = PathBuf::from(&payload.output_path);
    let chunk_size = payload.chunk_size.unwrap_or_else(|| client.upload_config().chunk_size);

    client
        .download_with_chunk_size(&payload.url, &output_path, chunk_size)
        .await
        .map_err(|e| ErrorResponse::from(e))?;

    Ok(Json(DownloadResponse {
        success: true,
        message: Some("Download completed successfully".to_string()),
    }))
}

/// List uploads handler
async fn list_uploads() -> Result<Json<ListUploadsResponse>, ErrorResponse> {
    let client = TusClient::new().map_err(|e| ErrorResponse::from(e))?;
    let uploads = client.list_incomplete().map_err(|e| ErrorResponse::from(e))?;
    let count = uploads.len();

    Ok(Json(ListUploadsResponse { uploads, count }))
}

/// Cleanup handler
async fn cleanup_handler(
    Json(payload): Json<CleanupRequest>,
) -> Result<Json<CleanupResponse>, ErrorResponse> {
    let client = TusClient::new().map_err(|e| ErrorResponse::from(e))?;
    let cleaned = client.cleanup(payload.days).map_err(|e| ErrorResponse::from(e))?;

    Ok(Json(CleanupResponse { cleaned }))
}

/// Terminate handler
async fn terminate_handler(
    Json(payload): Json<TerminateRequest>,
) -> Result<Json<TerminateResponse>, ErrorResponse> {
    let client = TusClient::new().map_err(|e| ErrorResponse::from(e))?;

    client
        .terminate_upload(&payload.upload_url)
        .await
        .map_err(|e| ErrorResponse::from(e))?;

    Ok(Json(TerminateResponse {
        success: true,
        message: Some("Upload terminated successfully".to_string()),
    }))
}

/// Create the API router
fn create_router() -> Router {
    // Enable CORS for all origins
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_check))
        .route("/info", get(server_info))
        .route("/upload", post(upload_handler))
        .route("/download", post(download_handler))
        .route("/uploads", get(list_uploads))
        .route("/cleanup", post(cleanup_handler))
        .route("/terminate", post(terminate_handler))
        .layer(cors)
}

/// Run the server
pub async fn run_server(config: ServerConfig) -> ZtusResult<()> {
    let app = create_router();

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| ZtusError::IoError(e))?;

    tracing::info!("ztus API server listening on http://{}", addr);
    tracing::info!("Available endpoints:");
    tracing::info!("  GET  /health   - Health check");
    tracing::info!("  GET  /info     - Server information");
    tracing::info!("  POST /upload   - Upload a file");
    tracing::info!("  POST /download - Download a file");
    tracing::info!("  GET  /uploads  - List incomplete uploads");
    tracing::info!("  POST /cleanup  - Clean up old uploads");
    tracing::info!("  POST /terminate - Terminate an upload");

    axum::serve(listener, app)
        .await
        .map_err(|e| ZtusError::IoError(e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upload_config_from_request() {
        let req = UploadConfigRequest {
            chunk_size: Some(10 * 1024 * 1024),
            max_retries: Some(5),
            timeout: Some(60),
            resume: Some(false),
            verify_checksum: Some(false),
            checksum_algorithm: Some("sha1".to_string()),
            headers: Some(vec![("X-Custom".to_string(), "value".to_string())]),
            metadata: Some(vec![("key".to_string(), "value".to_string())]),
            disable_adaptive: Some(true),
            adaptive_initial_chunk_size: Some(20 * 1024 * 1024),
            adaptive_min_chunk_size: Some(2 * 1024 * 1024),
            adaptive_max_chunk_size: Some(100 * 1024 * 1024),
        };

        let config: UploadConfig = req.into();

        assert_eq!(config.chunk_size, 10 * 1024 * 1024);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.timeout, 60);
        assert_eq!(config.resume, false);
        assert_eq!(config.verify_checksum, false);
        assert!(matches!(config.checksum_algorithm, crate::config::ChecksumAlgorithm::Sha1));
        assert_eq!(config.headers.len(), 1);
        assert_eq!(config.metadata.len(), 1);
        assert_eq!(config.adaptive.enabled, false);
        assert_eq!(config.adaptive.initial_chunk_size, 20 * 1024 * 1024);
        assert_eq!(config.adaptive.min_chunk_size, 2 * 1024 * 1024);
        assert_eq!(config.adaptive.max_chunk_size, 100 * 1024 * 1024);
    }

    #[test]
    fn test_default_cleanup_days() {
        assert_eq!(default_cleanup_days(), 7);
    }
}
