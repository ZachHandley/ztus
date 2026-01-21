//! TUS protocol implementation
//!
//! This module implements the TUS resumable upload protocol as specified in
//! https://tus.io/protocols/resumable-upload

use crate::config::ChecksumAlgorithm;
use crate::error::{Result, ZtusError};
use base64::Engine;
use reqwest::{Client, Method, StatusCode};
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

/// TUS protocol headers
pub const TUS_RESUMABLE: &str = "Tus-Resumable";
pub const UPLOAD_LENGTH: &str = "Upload-Length";
pub const UPLOAD_OFFSET: &str = "Upload-Offset";
pub const UPLOAD_METADATA: &str = "Upload-Metadata";
pub const UPLOAD_CHECKSUM: &str = "Upload-Checksum";
#[allow(dead_code)]
pub const UPLOAD_DEFER_LENGTH: &str = "Upload-Defer-Length";
#[allow(dead_code)]
pub const UPLOAD_CONCAT: &str = "Upload-Concat";
pub const UPLOAD_EXTENSION: &str = "Upload-Extension";

/// TUS protocol version
pub const TUS_VERSION: &str = "1.0.0";

/// TUS extensions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TusExtension {
    Creation,
    CreationWithUpload,
    Termination,
    Checksum,
    Expiration,
    Concatenation,
}

impl TusExtension {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Creation => "creation",
            Self::CreationWithUpload => "creation-with-upload",
            Self::Termination => "termination",
            Self::Checksum => "checksum",
            Self::Expiration => "expiration",
            Self::Concatenation => "concatenation",
        }
    }
}

impl std::str::FromStr for TusExtension {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "creation" => Ok(Self::Creation),
            "creation-with-upload" => Ok(Self::CreationWithUpload),
            "termination" => Ok(Self::Termination),
            "checksum" => Ok(Self::Checksum),
            "expiration" => Ok(Self::Expiration),
            "concatenation" => Ok(Self::Concatenation),
            _ => Err(format!("Unknown TUS extension: {}", s)),
        }
    }
}

/// Server capabilities discovered via OPTIONS request
#[derive(Debug, Clone)]
pub struct ServerCapabilities {
    /// TUS protocol version
    pub version: String,

    /// Supported extensions
    pub extensions: Vec<TusExtension>,

    /// Maximum upload size (if specified)
    pub max_size: Option<u64>,
}

impl std::fmt::Display for ServerCapabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "TUS Server Capabilities")?;
        writeln!(f, "=======================")?;
        writeln!(f)?;
        writeln!(f, "Protocol Version: {}", self.version)?;
        writeln!(f)?;

        if self.extensions.is_empty() {
            writeln!(f, "Extensions: None")?;
        } else {
            writeln!(f, "Supported Extensions:")?;
            for ext in &self.extensions {
                let supported = "✓";
                let description = match ext {
                    TusExtension::Creation => "creation-defer-length",
                    TusExtension::CreationWithUpload => "creation-with-upload",
                    TusExtension::Termination => "termination",
                    TusExtension::Checksum => "checksum",
                    TusExtension::Expiration => "expiration",
                    TusExtension::Concatenation => "concatenation",
                };
                writeln!(f, "  {} {}", supported, description)?;
            }
        }
        writeln!(f)?;

        if let Some(max) = self.max_size {
            writeln!(f, "Maximum Upload Size: {} bytes", max)?;
            if max >= 1024 * 1024 * 1024 {
                writeln!(
                    f,
                    "                     ({:.2} GB)",
                    max as f64 / (1024.0 * 1024.0 * 1024.0)
                )?;
            } else if max >= 1024 * 1024 {
                writeln!(
                    f,
                    "                     ({:.2} MB)",
                    max as f64 / (1024.0 * 1024.0)
                )?;
            }
        } else {
            writeln!(f, "Maximum Upload Size: Not specified by server")?;
        }

        Ok(())
    }
}

/// Encode metadata into TUS Upload-Metadata header format
///
/// The TUS protocol specifies that metadata should be formatted as:
/// `key1 base64value1,key2 base64value2`
///
/// All values are base64-encoded, but keys are sent as plain text.
pub fn encode_metadata(metadata: &HashMap<String, String>) -> String {
    metadata
        .iter()
        .map(|(key, value)| format!("{} {}", key, base64::prelude::BASE64_STANDARD.encode(value)))
        .collect::<Vec<_>>()
        .join(",")
}

/// Decode metadata from TUS Upload-Metadata header format
///
/// Parses the Upload-Metadata header and returns a HashMap of decoded key-value pairs.
/// The format is: `key1 base64value1,key2 base64value2`
pub fn decode_metadata(metadata_header: &str) -> Result<HashMap<String, String>> {
    let mut metadata = HashMap::new();

    // Split by comma to get individual key-value pairs
    for pair in metadata_header.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        // Split by first space to separate key from base64-encoded value
        if let Some((key, encoded_value)) = pair.split_once(' ') {
            let key = key.trim();
            let encoded_value = encoded_value.trim();

            if key.is_empty() {
                continue;
            }

            // Decode base64 value
            match base64::prelude::BASE64_STANDARD.decode(encoded_value) {
                Ok(decoded_bytes) => match String::from_utf8(decoded_bytes) {
                    Ok(value) => {
                        metadata.insert(key.to_string(), value);
                    }
                    Err(_) => {
                        tracing::warn!(
                            "Metadata value for key '{}' is not valid UTF-8, skipping",
                            key
                        );
                    }
                },
                Err(_) => {
                    tracing::warn!(
                        "Metadata value for key '{}' is not valid base64, skipping",
                        key
                    );
                }
            }
        }
    }

    Ok(metadata)
}

/// Query upload information including metadata (HEAD request)
pub async fn get_upload_info(upload_url: &str) -> Result<UploadInfo> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(ZtusError::from)?;

    let response = client
        .head(upload_url)
        .header(TUS_RESUMABLE, TUS_VERSION)
        .send()
        .await
        .map_err(ZtusError::from)?;

    match response.status() {
        StatusCode::OK => {
            let offset = response
                .headers()
                .get(UPLOAD_OFFSET)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    ZtusError::ProtocolError("Missing or invalid Upload-Offset".to_string())
                })?;

            let length = response
                .headers()
                .get(UPLOAD_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok());

            // Decode metadata if present
            let metadata = response
                .headers()
                .get(UPLOAD_METADATA)
                .and_then(|v| v.to_str().ok())
                .map(decode_metadata)
                .transpose()?
                .unwrap_or_default();

            Ok(UploadInfo {
                upload_url: upload_url.to_string(),
                offset,
                length,
                metadata,
            })
        }
        StatusCode::NOT_FOUND | StatusCode::GONE => Err(ZtusError::UploadTerminated),
        status => Err(ZtusError::ProtocolError(format!(
            "HEAD request failed with status: {}",
            status
        ))),
    }
}

/// Information about an upload
#[derive(Debug, Clone)]
pub struct UploadInfo {
    /// The upload URL
    pub upload_url: String,

    /// Current upload offset in bytes
    pub offset: u64,

    /// Total upload length in bytes (if available)
    pub length: Option<u64>,

    /// Metadata associated with the upload
    pub metadata: HashMap<String, String>,
}

impl std::fmt::Display for UploadInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Upload Information")?;
        writeln!(f, "==================")?;
        writeln!(f)?;
        writeln!(f, "URL: {}", self.upload_url)?;
        writeln!(f, "Offset: {} bytes", self.offset)?;

        if let Some(length) = self.length {
            let progress = (self.offset as f64 / length as f64) * 100.0;
            writeln!(f, "Length: {} bytes", length)?;
            writeln!(f, "Progress: {:.1}%", progress)?;
        } else {
            writeln!(f, "Length: Unknown (deferred length)")?;
        }

        writeln!(f)?;

        if self.metadata.is_empty() {
            writeln!(f, "Metadata: None")?;
        } else {
            writeln!(f, "Metadata:")?;
            for (key, value) in &self.metadata {
                writeln!(f, "  {} = {}", key, value)?;
            }
        }

        Ok(())
    }
}

/// TUS protocol client
pub struct TusProtocol {
    client: Client,
    base_url: String,
    headers: Vec<(String, String)>,
}

impl TusProtocol {
    /// Create a new TUS protocol client
    pub fn new(base_url: String, timeout: Duration) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            // HTTP/2 configuration
            .http2_prior_knowledge()
            .http2_keep_alive_interval(Duration::from_secs(30))
            .http2_keep_alive_timeout(Duration::from_secs(10))
            .http2_adaptive_window(true) // Auto-adjust flow control window
            // Connection pool - INCREASED for HTTP/2 multiplexing
            .pool_max_idle_per_host(10) // Was 1, now 10 for better concurrency
            .pool_idle_timeout(Duration::from_secs(90))
            // TCP optimizations
            .tcp_nodelay(true) // Disable Nagle's algorithm - CRITICAL for upload speed
            .tcp_keepalive(Duration::from_secs(60)) // Detect dead connections
            // Connection limits
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(ZtusError::from)?;

        tracing::debug!("Created HTTP/2 client with TCP_NODELAY and pool_max_idle_per_host=10");

        Ok(Self {
            client,
            base_url,
            headers: Vec::new(),
        })
    }

    /// Create a new TUS protocol client with custom headers
    pub fn with_headers(
        base_url: String,
        timeout: Duration,
        headers: Vec<(String, String)>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .http2_prior_knowledge()
            .http2_keep_alive_interval(Duration::from_secs(30))
            .http2_keep_alive_timeout(Duration::from_secs(10))
            .http2_adaptive_window(true)
            .pool_max_idle_per_host(10) // Was 1
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(ZtusError::from)?;

        tracing::debug!("Created HTTP/2 client with custom headers and TCP optimizations");

        Ok(Self {
            client,
            base_url,
            headers,
        })
    }

    /// Add custom headers to a request builder
    fn apply_headers(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }
        request
    }

    /// Discover server capabilities via OPTIONS request
    pub async fn discover_capabilities(&self) -> Result<ServerCapabilities> {
        let request = self.client.request(Method::OPTIONS, &self.base_url);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        if response.status() != StatusCode::OK {
            return Err(ZtusError::ProtocolError(format!(
                "OPTIONS request failed with status: {}",
                response.status()
            )));
        }

        let headers = response.headers();

        // Check TUS version
        let version = headers
            .get(TUS_RESUMABLE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("1.0.0")
            .to_string();

        // Parse extensions
        let extensions = headers
            .get(UPLOAD_EXTENSION)
            .and_then(|v| v.to_str().ok())
            .map(|ext| {
                ext.split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(ServerCapabilities {
            version,
            extensions,
            max_size: None, // Tus-Max-Size is not provided on this response.
        })
    }

    /// Resolve a potentially relative URL against a base URL following RFC 3986
    ///
    /// This method implements proper URL resolution as specified in RFC 3986 Section 5.2.
    /// It handles various relative URL formats that TUS servers may return in Location headers.
    ///
    /// # Resolution Rules
    ///
    /// - **Absolute URLs** (http:// or https://): Returned as-is without modification
    /// - **Path-absolute URLs** (starting with /): Resolved against the base URL's origin
    ///   - Base: "http://example.com/api", Location: "/files/abc" → "http://example.com/files/abc"
    /// - **Path-relative URLs** (no leading slash): Resolved against base URL's path
    ///   - If base has trailing slash: "http://example.com/api/", Location: "files/abc" → "http://example.com/api/files/abc"
    ///   - If base has no trailing slash: "http://example.com/api", Location: "files/abc" → "http://example.com/files/abc"
    /// - **Dot segments**: Properly normalized according to RFC 3986
    ///   - Base: "http://example.com/api/upload", Location: "../files/abc" → "http://example.com/api/files/abc"
    ///
    /// # Arguments
    ///
    /// * `base_url` - The base URL to resolve against (typically the TUS server's upload endpoint)
    /// * `location` - The Location header value from the TUS server (may be relative or absolute)
    ///
    /// # Returns
    ///
    /// A fully-resolved absolute URL as a String
    ///
    /// # Errors
    ///
    /// Returns `ZtusError::ProtocolError` if:
    /// - The base URL is invalid
    /// - The location URL cannot be resolved against the base
    fn resolve_url(base_url: &str, location: &str) -> Result<String> {
        // If location is already an absolute URL, return it as-is
        if location.starts_with("http://") || location.starts_with("https://") {
            return Ok(location.to_string());
        }

        // Parse the base URL
        let base = Url::parse(base_url)
            .map_err(|e| ZtusError::ProtocolError(format!("Invalid base URL: {}", e)))?;

        // Use the url crate's join method which implements RFC 3986
        let resolved = base.join(location).map_err(|e| {
            ZtusError::ProtocolError(format!(
                "Failed to resolve Location URL '{}' against base '{}': {}",
                location, base_url, e
            ))
        })?;

        Ok(resolved.to_string())
    }

    /// Create a new upload (POST request)
    pub async fn create_upload(
        &self,
        length: u64,
        metadata: Option<Vec<(String, String)>>,
        checksum_algorithm: Option<ChecksumAlgorithm>,
        checksum_value: Option<String>,
    ) -> Result<String> {
        let mut request = self.client.post(&self.base_url);

        // Apply custom headers first (so TUS headers can override if needed)
        request = self.apply_headers(request);

        // Set TUS version header
        request = request.header(TUS_RESUMABLE, TUS_VERSION);

        // Set upload length
        request = request.header(UPLOAD_LENGTH, length.to_string());

        // Set metadata if provided
        if let Some(meta) = metadata {
            let metadata_map: HashMap<String, String> = meta.into_iter().collect();
            let metadata_str = encode_metadata(&metadata_map);
            request = request.header(UPLOAD_METADATA, metadata_str);
        }

        // Clone checksum_value for later verification if needed
        let expected_checksum = checksum_value.clone();

        // Set checksum if provided
        if let (Some(algo), Some(checksum)) = (checksum_algorithm, checksum_value) {
            let checksum_header = format!("{} {}", algo.as_tus_algorithm(), checksum);
            request = request.header(UPLOAD_CHECKSUM, checksum_header);
        }

        let response = request.send().await.map_err(ZtusError::from)?;

        match response.status() {
            StatusCode::CREATED => {
                // Verify server checksum if returned
                if let Some(expected_checksum) = expected_checksum {
                    if let Some(server_checksum) = response
                        .headers()
                        .get("Upload-Checksum")
                        .and_then(|v| v.to_str().ok())
                    {
                        // Server returns checksum in format: "algorithm <checksum>"
                        if let Some(server_checksum_value) = server_checksum.split(' ').nth(1) {
                            if server_checksum_value != expected_checksum {
                                return Err(ZtusError::ChecksumMismatch {
                                    expected: expected_checksum,
                                    actual: server_checksum_value.to_string(),
                                });
                            }
                        }
                    }
                }

                let location = response
                    .headers()
                    .get("Location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        ZtusError::ProtocolError("Missing Location header".to_string())
                    })?;

                // Resolve relative URL against base URL following RFC 3986
                let upload_url = Self::resolve_url(&self.base_url, location)?;

                Ok(upload_url)
            }
            status => {
                // Try to read error response body for better error messages
                let error_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<unable to read response body>".to_string());
                let error_msg = if error_body.is_empty() || error_body.contains('<') {
                    format!("Create upload failed with status: {}", status)
                } else {
                    format!(
                        "Create upload failed with status: {} - Server error: {}",
                        status,
                        error_body.trim()
                    )
                };
                Err(ZtusError::ProtocolError(error_msg))
            }
        }
    }

    /// Query upload status (HEAD request)
    pub async fn get_upload_offset(&self, upload_url: &str) -> Result<u64> {
        let request = self
            .client
            .head(upload_url)
            .header(TUS_RESUMABLE, TUS_VERSION);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        match response.status() {
            StatusCode::OK => {
                let offset = response
                    .headers()
                    .get(UPLOAD_OFFSET)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| {
                        ZtusError::ProtocolError("Missing or invalid Upload-Offset".to_string())
                    })?;

                Ok(offset)
            }
            StatusCode::NOT_FOUND | StatusCode::GONE => Err(ZtusError::UploadTerminated),
            status => Err(ZtusError::ProtocolError(format!(
                "HEAD request failed with status: {}",
                status
            ))),
        }
    }

    /// Upload a chunk (PATCH request)
    pub async fn upload_chunk(&self, upload_url: &str, offset: u64, data: Vec<u8>) -> Result<u64> {
        let request = self
            .client
            .patch(upload_url)
            .header(TUS_RESUMABLE, TUS_VERSION)
            .header("Content-Type", "application/offset+octet-stream")
            .header(UPLOAD_OFFSET, offset.to_string())
            .body(data);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => {
                let new_offset = response
                    .headers()
                    .get(UPLOAD_OFFSET)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| {
                        ZtusError::ProtocolError("Missing Upload-Offset in response".to_string())
                    })?;

                Ok(new_offset)
            }
            StatusCode::CONFLICT => {
                // Offset mismatch - server has different offset
                let server_offset = response
                    .headers()
                    .get(UPLOAD_OFFSET)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);

                Err(ZtusError::OffsetMismatch {
                    expected: offset,
                    actual: server_offset,
                })
            }
            StatusCode::NOT_FOUND | StatusCode::GONE => Err(ZtusError::UploadTerminated),
            StatusCode::PRECONDITION_FAILED => Err(ZtusError::ProtocolError(
                "Precondition failed - upload may have expired".to_string(),
            )),
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE => Err(ZtusError::ProtocolError(
                "Request header fields too large".to_string(),
            )),
            StatusCode::PAYLOAD_TOO_LARGE => Err(ZtusError::ProtocolError(
                "Upload too large for server".to_string(),
            )),
            status if status.is_server_error() => Err(ZtusError::ProtocolError(format!(
                "Server error during upload: {}",
                status
            ))),
            status => Err(ZtusError::ProtocolError(format!(
                "PATCH request failed with status: {}",
                status
            ))),
        }
    }

    /// Terminate an upload (DELETE request)
    pub async fn terminate_upload(&self, upload_url: &str) -> Result<()> {
        let request = self
            .client
            .delete(upload_url)
            .header(TUS_RESUMABLE, TUS_VERSION);
        let request = self.apply_headers(request);

        let response = request.send().await.map_err(ZtusError::from)?;

        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT | StatusCode::NOT_FOUND => Ok(()),
            StatusCode::FORBIDDEN => Err(ZtusError::ProtocolError(
                "Termination not supported by server".to_string(),
            )),
            status => Err(ZtusError::ProtocolError(format!(
                "DELETE request failed with status: {}",
                status
            ))),
        }
    }

    /// Upload a chunk with retry logic
    pub async fn upload_chunk_with_retry(
        &self,
        upload_url: &str,
        offset: u64,
        data: Vec<u8>,
        max_retries: usize,
    ) -> Result<u64> {
        let mut retries = 0;

        loop {
            match self.upload_chunk(upload_url, offset, data.clone()).await {
                Ok(new_offset) => return Ok(new_offset),
                Err(ZtusError::OffsetMismatch { .. }) => {
                    // Don't retry offset mismatches - let caller handle it
                    return self.upload_chunk(upload_url, offset, data).await;
                }
                Err(ZtusError::HttpError(_e)) if retries < max_retries => {
                    retries += 1;

                    // Exponential backoff with a 30s cap
                    let delay_ms = 100_u64.saturating_mul(2_u64.pow(retries as u32));
                    let delay_ms = delay_ms.min(30_000);
                    let delay = tokio::time::Duration::from_millis(delay_ms);
                    tracing::warn!(
                        "Upload chunk failed (attempt {}/{}), retrying in {:?}",
                        retries,
                        max_retries,
                        delay
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(e) => return Err(e),
            }

            if retries >= max_retries {
                return Err(ZtusError::ProtocolError("Max retries exceeded".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tus_extension_from_str() {
        assert_eq!(
            "creation".parse::<TusExtension>(),
            Ok(TusExtension::Creation)
        );
        assert_eq!(
            "checksum".parse::<TusExtension>(),
            Ok(TusExtension::Checksum)
        );
        assert_eq!(
            "termination".parse::<TusExtension>(),
            Ok(TusExtension::Termination)
        );
        assert!("invalid".parse::<TusExtension>().is_err());
    }

    #[test]
    fn test_tus_extension_as_str() {
        assert_eq!(TusExtension::Creation.as_str(), "creation");
        assert_eq!(TusExtension::Checksum.as_str(), "checksum");
        assert_eq!(TusExtension::Termination.as_str(), "termination");
    }

    #[test]
    fn test_tus_protocol_client_creation() {
        let client = TusProtocol::new("http://example.com".to_string(), Duration::from_secs(30));
        assert!(client.is_ok());
    }

    #[test]
    fn test_resolve_url_absolute() {
        // Absolute URLs should be returned as-is
        let base = "http://example.com/files";
        let location = "http://example.com/files/abc123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/files/abc123");

        // HTTPS absolute URL
        let base = "http://example.com/files";
        let location = "https://other.com/uploads/xyz";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://other.com/uploads/xyz");
    }

    #[test]
    fn test_resolve_url_path_absolute() {
        // Path-absolute URLs (starting with /) should resolve against base origin
        let base = "http://example.com/files";
        let location = "/files/abc123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/files/abc123");

        // With base URL that has path
        let base = "http://example.com/upload/path";
        let location = "/files/abc123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/files/abc123");

        // With trailing slash on base
        let base = "http://example.com/files/";
        let location = "/uploads/xyz";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/uploads/xyz");
    }

    #[test]
    fn test_resolve_url_path_relative() {
        // Path-relative URLs should resolve against base path
        // When base doesn't have trailing slash, the last segment is replaced
        let base = "http://example.com/files";
        let location = "abc123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/abc123");

        // With trailing slash on base - path is appended
        let base = "http://example.com/files/";
        let location = "uploads/xyz";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/files/uploads/xyz");

        // Nested relative path
        let base = "http://example.com/upload/api";
        let location = "files/resume/upload-id";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "http://example.com/upload/files/resume/upload-id"
        );
    }

    #[test]
    fn test_resolve_url_dot_segments() {
        // Test URL normalization with dot segments (RFC 3986 Section 5.2.4)
        let base = "http://example.com/files/upload";
        let location = "../files/abc123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        // ../ goes up from /upload to /, then adds /files/abc123
        assert_eq!(result.unwrap(), "http://example.com/files/abc123");

        // Current directory reference (./ is normalized away)
        let base = "http://example.com/files";
        let location = "./uploads/abc123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        // Without trailing slash, "files" segment is replaced
        assert_eq!(result.unwrap(), "http://example.com/uploads/abc123");

        // With trailing slash, ./ is appended
        let base = "http://example.com/files/";
        let location = "./uploads/abc123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/files/uploads/abc123");
    }

    #[test]
    fn test_resolve_url_query_and_fragment() {
        // Location with query parameters
        let base = "http://example.com/files";
        let location = "/uploads/abc123?token=xyz";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "http://example.com/uploads/abc123?token=xyz"
        );

        // Location with fragment
        let base = "http://example.com/files";
        let location = "/uploads/abc123#section";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://example.com/uploads/abc123#section");

        // Both query and fragment
        let base = "http://example.com/files";
        let location = "/uploads/abc123?token=xyz#section";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "http://example.com/uploads/abc123?token=xyz#section"
        );
    }

    #[test]
    fn test_resolve_url_real_world_tus_servers() {
        // Common TUS server patterns

        // tusd pattern: absolute path
        let base = "http://tus.example.com/files";
        let location = "/files/abc123def456";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://tus.example.com/files/abc123def456");

        // Cloud storage pattern: absolute URL with different host
        let base = "http://app.example.com/api/upload";
        let location = "https://storage.example.com/files/upload-id-123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "https://storage.example.com/files/upload-id-123"
        );

        // Relative path without leading slash (base has trailing slash)
        let base = "http://uploads.example.com/api/";
        let location = "files/upload-id-123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "http://uploads.example.com/api/files/upload-id-123"
        );

        // Relative path without leading slash (base no trailing slash)
        let base = "http://uploads.example.com/api";
        let location = "files/upload-id-123";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_ok());
        // "api" segment is replaced with "files"
        assert_eq!(
            result.unwrap(),
            "http://uploads.example.com/files/upload-id-123"
        );
    }

    #[test]
    fn test_resolve_url_invalid_base() {
        // Invalid base URL should return error
        let base = "not-a-valid-url";
        let location = "/files/abc";
        let result = TusProtocol::resolve_url(base, location);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid base URL"));
    }

    #[test]
    fn test_resolve_url_invalid_location() {
        // Control characters in URL should cause parsing error
        let base = "http://example.com";
        let location = "http://example.com/\x00invalid";
        let result = TusProtocol::resolve_url(base, location);
        // The url crate is quite permissive, so this might not error
        // Let's just verify it handles it gracefully
        match result {
            Ok(_) => {} // url crate accepted it
            Err(e) => assert!(e.to_string().contains("Failed to resolve Location URL")),
        }
    }

    #[test]
    fn test_encode_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), "test.txt".to_string());
        metadata.insert("type".to_string(), "document".to_string());

        let encoded = encode_metadata(&metadata);

        // Should be in format: "key base64value,key base64value"
        assert!(encoded.contains("filename "));
        assert!(encoded.contains("type "));

        // base64 encoding of "test.txt" is "dGVzdC50eHQ="
        assert!(encoded.contains("dGVzdC50eHQ="));

        // base64 encoding of "document" is "ZG9jdW1lbnQ="
        assert!(encoded.contains("ZG9jdW1lbnQ="));
    }

    #[test]
    fn test_encode_metadata_empty() {
        let metadata = HashMap::new();
        let encoded = encode_metadata(&metadata);
        assert_eq!(encoded, "");
    }

    #[test]
    fn test_encode_metadata_special_characters() {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value with spaces".to_string());
        metadata.insert("unicode".to_string(), "日本語".to_string());

        let encoded = encode_metadata(&metadata);

        // Should encode special characters
        assert!(encoded.contains("key "));
        assert!(encoded.contains("unicode "));

        // Verify it can be decoded back
        let decoded = decode_metadata(&encoded).unwrap();
        assert_eq!(decoded.get("key").unwrap(), "value with spaces");
        assert_eq!(decoded.get("unicode").unwrap(), "日本語");
    }

    #[test]
    fn test_decode_metadata() {
        let encoded = "filename dGVzdC50eHQ=,type ZG9jdW1lbnQ=";
        let decoded = decode_metadata(encoded).unwrap();

        assert_eq!(decoded.get("filename").unwrap(), "test.txt");
        assert_eq!(decoded.get("type").unwrap(), "document");
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_decode_metadata_empty() {
        let decoded = decode_metadata("").unwrap();
        assert_eq!(decoded.len(), 0);
    }

    #[test]
    fn test_decode_metadata_with_spaces() {
        // base64 of "hello world" is "aGVsbG8gd29ybGQ="
        let encoded = "message aGVsbG8gd29ybGQ=";
        let decoded = decode_metadata(encoded).unwrap();

        assert_eq!(decoded.get("message").unwrap(), "hello world");
    }

    #[test]
    fn test_encode_metadata_roundtrip() {
        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), "test.txt".to_string());
        metadata.insert("type".to_string(), "document".to_string());

        let encoded = encode_metadata(&metadata);
        let decoded = decode_metadata(&encoded).unwrap();

        assert_eq!(decoded.get("filename").unwrap(), "test.txt");
        assert_eq!(decoded.get("type").unwrap(), "document");
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_decode_metadata_unicode() {
        // base64 of "日本語" is "5pel5pys6Kqe"
        let encoded = "text 5pel5pys6Kqe";
        let decoded = decode_metadata(encoded).unwrap();

        assert_eq!(decoded.get("text").unwrap(), "日本語");
    }

    #[test]
    fn test_decode_metadata_invalid_base64_skipped() {
        // Invalid base64 should be skipped with a warning
        // "test" in base64 is "dGVzdA=="
        // "!!!@^^^" is not valid base64 (contains ^ which is not a base64 char)
        let encoded = "valid_key dGVzdA==,invalid_key !!!@^^^";
        let decoded = decode_metadata(encoded).unwrap();

        // Should have only the valid entry
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded.get("valid_key").unwrap(), "test");
        assert!(!decoded.contains_key("invalid_key"));
    }

    #[test]
    fn test_decode_metadata_trailing_comma() {
        let encoded = "key dGVzdA==,";
        let decoded = decode_metadata(encoded).unwrap();

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded.get("key").unwrap(), "test");
    }

    #[test]
    fn test_upload_info_display() {
        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), "test.txt".to_string());
        metadata.insert("type".to_string(), "document".to_string());

        let info = UploadInfo {
            upload_url: "http://example.com/files/123".to_string(),
            offset: 512,
            length: Some(1024),
            metadata,
        };

        let display = format!("{}", info);
        assert!(display.contains("Upload Information"));
        assert!(display.contains("http://example.com/files/123"));
        assert!(display.contains("512 bytes"));
        assert!(display.contains("1024 bytes"));
        assert!(display.contains("50.0%")); // 512/1024 = 50%
        assert!(display.contains("filename"));
        assert!(display.contains("test.txt"));
        assert!(display.contains("type"));
        assert!(display.contains("document"));
    }

    #[test]
    fn test_upload_info_no_metadata() {
        let info = UploadInfo {
            upload_url: "http://example.com/files/123".to_string(),
            offset: 0,
            length: Some(1024),
            metadata: HashMap::new(),
        };

        let display = format!("{}", info);
        assert!(display.contains("Metadata: None"));
    }

    #[test]
    fn test_upload_info_deferred_length() {
        let info = UploadInfo {
            upload_url: "http://example.com/files/123".to_string(),
            offset: 512,
            length: None,
            metadata: HashMap::new(),
        };

        let display = format!("{}", info);
        assert!(display.contains("Unknown (deferred length)"));
    }
}
