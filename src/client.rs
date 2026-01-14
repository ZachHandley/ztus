//! High-level TUS client
//!
//! This module provides a simplified API for TUS operations.

use crate::config::{AppConfig, UploadConfig};
use crate::download::DownloadManager;
use crate::error::Result;
use crate::upload::UploadManager;
use std::path::{Path, PathBuf};

/// TUS client for uploads and downloads
pub struct TusClient {
    config: AppConfig,
    state_dir: PathBuf,
}

impl TusClient {
    /// Create a new TUS client with default configuration
    pub fn new() -> Result<Self> {
        let config = AppConfig::load()?;
        let state_dir = config.state_dir.join("state");

        // Ensure state directory exists
        std::fs::create_dir_all(&state_dir)
            .map_err(crate::error::ZtusError::from)?;

        Ok(Self {
            config,
            state_dir,
        })
    }

    /// Create a new TUS client with custom configuration
    #[allow(dead_code)]
    pub fn with_config(config: AppConfig) -> Result<Self> {
        let state_dir = config.state_dir.join("state");

        // Ensure state directory exists
        std::fs::create_dir_all(&state_dir)
            .map_err(crate::error::ZtusError::from)?;

        Ok(Self {
            config,
            state_dir,
        })
    }

    /// Upload a file to a TUS endpoint
    #[allow(dead_code)]
    pub async fn upload(&self, file_path: &Path, upload_url: &str) -> Result<()> {
        self.upload_with_config(file_path, upload_url, &self.config.upload).await
    }

    /// Upload a file to a TUS endpoint with custom configuration
    pub async fn upload_with_config(
        &self,
        file_path: &Path,
        upload_url: &str,
        upload_config: &UploadConfig,
    ) -> Result<()> {
        let manager = UploadManager::new(
            upload_url.to_string(),
            upload_config.clone(),
            self.state_dir.clone(),
        )?;

        manager.upload_file(file_path).await
    }

    /// Resume an incomplete upload
    #[allow(dead_code)]
    pub async fn resume(&self, file_path: &Path, upload_url: &str) -> Result<()> {
        self.resume_with_config(file_path, upload_url, &self.config.upload).await
    }

    /// Resume an incomplete upload with custom configuration
    pub async fn resume_with_config(
        &self,
        file_path: &Path,
        upload_url: &str,
        upload_config: &UploadConfig,
    ) -> Result<()> {
        let manager = UploadManager::new(
            upload_url.to_string(),
            upload_config.clone(),
            self.state_dir.clone(),
        )?;

        manager.resume_upload(file_path).await
    }

    /// Download a file from a URL
    #[allow(dead_code)]
    pub async fn download(&self, url: &str, output_path: &Path) -> Result<()> {
        self.download_with_chunk_size(url, output_path, self.config.upload.chunk_size)
            .await
    }

    /// Download a file from a URL with custom chunk size
    pub async fn download_with_chunk_size(
        &self,
        url: &str,
        output_path: &Path,
        chunk_size: usize,
    ) -> Result<()> {
        let manager = DownloadManager::new(chunk_size)?;

        manager.download_file(url, output_path).await
    }

    /// List incomplete uploads
    pub fn list_incomplete(&self) -> Result<Vec<String>> {
        let upload_config = self.config.upload.clone();

        let manager = UploadManager::new(
            "dummy".to_string(), // URL not needed for listing
            upload_config,
            self.state_dir.clone(),
        )?;

        let states = manager.list_incomplete()?;

        Ok(states
            .into_iter()
            .map(|s| s.to_string())
            .collect())
    }

    /// List incomplete uploads, returning state objects for custom formatting
    #[allow(dead_code)]
    pub fn list_incomplete_states(&self) -> Result<Vec<crate::storage::UploadState>> {
        let upload_config = self.config.upload.clone();

        let manager = UploadManager::new(
            "dummy".to_string(), // URL not needed for listing
            upload_config,
            self.state_dir.clone(),
        )?;

        manager.list_incomplete()
    }

    /// Clean up old incomplete uploads
    pub fn cleanup(&self, days: i64) -> Result<usize> {
        let upload_config = self.config.upload.clone();

        let manager = UploadManager::new(
            "dummy".to_string(), // URL not needed for cleanup
            upload_config,
            self.state_dir.clone(),
        )?;

        manager.cleanup(days)
    }

    /// Get the state directory path
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Get the upload configuration
    pub fn upload_config(&self) -> &UploadConfig {
        &self.config.upload
    }

    /// Discover server capabilities
    pub async fn discover_capabilities(&self, url: &str) -> Result<crate::protocol::ServerCapabilities> {
        use std::time::Duration;
        let protocol = crate::protocol::TusProtocol::new(
            url.to_string(),
            Duration::from_secs(self.config.upload.timeout),
        )?;
        protocol.discover_capabilities().await
    }

    /// Terminate an upload at the given URL
    pub async fn terminate_upload(&self, upload_url: &str) -> Result<()> {
        use std::time::Duration;
        // Extract base URL from upload URL for protocol client creation
        let base_url = self.extract_base_url(upload_url);
        let protocol = crate::protocol::TusProtocol::new(
            base_url,
            Duration::from_secs(self.config.upload.timeout),
        )?;
        protocol.terminate_upload(upload_url).await
    }

    /// Query upload information including metadata
    pub async fn get_upload_info(&self, upload_url: &str) -> Result<crate::protocol::UploadInfo> {
        crate::protocol::get_upload_info(upload_url).await
    }

    /// Extract base URL from an upload URL
    fn extract_base_url(&self, url: &str) -> String {
        // Simple extraction: if URL contains path, get the base
        if let Some(pos) = url.find("/files") {
            url[..pos].to_string()
        } else if let Some(pos) = url.find("/upload") {
            url[..pos].to_string()
        } else {
            // Fallback: remove last path segment
            if let Some(last_slash) = url.rfind('/') {
                if last_slash > 8 { // Ensure we don't cut into http://
                    url[..last_slash].to_string()
                } else {
                    url.to_string()
                }
            } else {
                url.to_string()
            }
        }
    }
}

impl Default for TusClient {
    fn default() -> Self {
        Self::new().expect("Failed to create TUS client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = TusClient::new();
        assert!(client.is_ok());
    }
}
