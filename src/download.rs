//! Download support for ztus
//!
//! This module provides HTTP Range download functionality with resume support.

use crate::error::{Result, ZtusError};
use crate::storage::{DownloadState, StateStorage};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};

/// Download manager
pub struct DownloadManager {
    client: reqwest::Client,
    chunk_size: usize,
    storage: StateStorage,
    max_retries: usize,
}

impl DownloadManager {
    /// Create a new download manager
    pub fn new(chunk_size: usize) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(ZtusError::from)?;

        // Get state directory from environment or use default
        let state_dir = dirs::state_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap())
            .join("ztus");

        let storage = StateStorage::new(state_dir)?;

        Ok(Self {
            client,
            chunk_size,
            storage,
            max_retries: 3,
        })
    }

    /// Set max retries for download operations
    #[allow(dead_code)]
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Download a file with resume support
    pub async fn download_file(&self, url: &str, output_path: &Path) -> Result<()> {
        tracing::info!("Starting download: {} -> {}", url, output_path.display());

        // Check for existing download state
        let states = self.storage.list_download_states()?;
        let existing_state = states
            .into_iter()
            .find(|s| s.url == url && s.output_path == *output_path);

        let (start_offset, total_size, state_id) = match existing_state {
            Some(state) => {
                if state.is_complete() {
                    tracing::info!("Download already complete: {}", output_path.display());
                    return Ok(());
                }

                // Verify file hasn't changed on server
                let server_size = self.get_remote_size(url).await?;
                if server_size != state.total_size {
                    tracing::warn!(
                        "Remote file size changed (was {}, now {}), restarting download",
                        state.total_size,
                        server_size
                    );
                    self.start_new_download(url, output_path, server_size).await?
                } else {
                    // Verify local file size matches state offset
                    let local_size = if output_path.exists() {
                        std::fs::metadata(output_path)?.len()
                    } else {
                        0
                    };

                    if local_size != state.offset {
                        tracing::warn!(
                            "Local file size mismatch (state: {}, actual: {}), using actual size",
                            state.offset,
                            local_size
                        );
                        (local_size, state.total_size, state.id)
                    } else {
                        tracing::info!("Resuming download from offset: {}", state.offset);
                        (state.offset, state.total_size, state.id)
                    }
                }
            }
            None => {
                let total_size = self.get_remote_size(url).await?;
                self.start_new_download(url, output_path, total_size).await?
            }
        };

        // Check if file is already complete
        if start_offset >= total_size {
            tracing::info!("File already downloaded: {}", output_path.display());
            // Clean up state if exists
            if !state_id.is_empty() {
                let _ = self.storage.delete_download_state(&state_id);
            }
            return Ok(());
        }

        // Perform the download with retry logic
        self.download_with_retry(url, output_path, start_offset, total_size, &state_id)
            .await?;

        // Clean up state file on successful completion
        if !state_id.is_empty() {
            self.storage.delete_download_state(&state_id)?;
            tracing::debug!("Deleted download state: {}", state_id);
        }

        tracing::info!("File downloaded successfully: {}", output_path.display());

        Ok(())
    }

    /// Start a new download session
    async fn start_new_download(
        &self,
        url: &str,
        output_path: &Path,
        total_size: u64,
    ) -> Result<(u64, u64, String)> {
        let start_offset = if output_path.exists() {
            std::fs::metadata(output_path)?.len()
        } else {
            0
        };

        // Create download state
        let state = DownloadState::new(
            url.to_string(),
            output_path.to_path_buf(),
            total_size,
            self.chunk_size,
        );

        let state_id = state.id.clone();
        self.storage.save_download_state(&state)?;

        Ok((start_offset, total_size, state_id))
    }

    /// Get remote file size via HEAD request
    async fn get_remote_size(&self, url: &str) -> Result<u64> {
        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(ZtusError::from)?;

        if !response.status().is_success() {
            return Err(ZtusError::ProtocolError(format!(
                "HEAD request failed with status: {}",
                response.status()
            )));
        }

        let total_size = response
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                ZtusError::ProtocolError("Cannot determine file size (missing Content-Length)".to_string())
            })?;

        Ok(total_size)
    }

    /// Download with retry logic
    async fn download_with_retry(
        &self,
        url: &str,
        output_path: &Path,
        mut start_offset: u64,
        total_size: u64,
        state_id: &str,
    ) -> Result<()> {
        let mut retries = 0;

        loop {
            match self
                .download_range(url, output_path, start_offset, total_size, state_id)
                .await
            {
                Ok(_) => break Ok(()),
                Err(e) if retries < self.max_retries => {
                    retries += 1;
                    tracing::warn!(
                        "Download error (attempt {}/{}): {}",
                        retries,
                        self.max_retries,
                        e
                    );

                    // Check current file size before retrying
                    if output_path.exists() {
                        start_offset = std::fs::metadata(output_path)?.len();
                    }

                    tokio::time::sleep(Duration::from_secs(2u64.pow(retries as u32))).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Download a range of bytes with progress tracking
    async fn download_range(
        &self,
        url: &str,
        output_path: &Path,
        start_offset: u64,
        total_size: u64,
        state_id: &str,
    ) -> Result<()> {
        // Check if server supports range requests
        let supports_range = self.check_range_support(url).await?;
        if !supports_range && start_offset > 0 {
            return Err(ZtusError::ProtocolError(
                "Server does not support range requests, cannot resume".to_string(),
            ));
        }

        // Create progress bar
        let progress = ProgressBar::new(total_size);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .map_err(|e| ZtusError::ConfigError(e.to_string()))?
                .progress_chars("##-")
        );
        progress.set_position(start_offset);

        // Open file for appending (create if doesn't exist)
        let mut file = File::options()
            .create(true)
            .append(true)
            .open(output_path)
            .await
            .map_err(ZtusError::from)?;

        let mut current_offset = start_offset;

        // Download in chunks
        loop {
            if current_offset >= total_size {
                break;
            }

            // Calculate chunk size (don't exceed remaining bytes)
            let chunk_end = std::cmp::min(
                current_offset + self.chunk_size as u64 - 1,
                total_size - 1,
            );
            let range_header = format!("bytes={}-{}", current_offset, chunk_end);

            // Make request with Range header
            let response = self
                .client
                .get(url)
                .header("Range", range_header)
                .send()
                .await
                .map_err(ZtusError::from)?;

            let status = response.status();

            // Handle 416 Range Not Satisfiable (file already complete)
            if status == 416 {
                tracing::info!("Server indicates range is not satisfiable (file may be complete)");
                break;
            }

            // Handle 206 Partial Content (successful range request)
            if status != 206 && status != 200 {
                return Err(ZtusError::ProtocolError(format!(
                    "Unexpected status code: {}",
                    status
                )));
            }

            // Get content length for this chunk
            let content_length = response
                .headers()
                .get("Content-Length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            if content_length == 0 && current_offset < total_size {
                return Err(ZtusError::ProtocolError(
                    "Received empty response for non-final chunk".to_string(),
                ));
            }

            // Stream bytes to file
            let mut stream = response.bytes_stream();
            let mut writer = BufWriter::new(&mut file);

            use futures_util::StreamExt;
            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(ZtusError::from)?;
                writer.write_all(&chunk).await.map_err(ZtusError::from)?;
                current_offset += chunk.len() as u64;
                progress.set_position(current_offset);
            }

            writer.flush().await.map_err(ZtusError::from)?;

            // Update state if we have a state_id
            if !state_id.is_empty() {
                let mut state = self.storage.load_download_state(state_id)?;
                state.update_offset(current_offset);
                self.storage.save_download_state(&state)?;
            }

            // Small yield to prevent task starvation
            tokio::task::yield_now().await;
        }

        progress.finish_with_message("Download complete!");

        Ok(())
    }

    /// Check if server supports range requests
    async fn check_range_support(&self, url: &str) -> Result<bool> {
        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(ZtusError::from)?;

        let accept_ranges = response
            .headers()
            .get("Accept-Ranges")
            .and_then(|v| v.to_str().ok())
            .map(|s| s == "bytes")
            .unwrap_or(false);

        Ok(accept_ranges)
    }

    /// List incomplete downloads
    #[allow(dead_code)]
    pub fn list_incomplete(&self) -> Result<Vec<DownloadState>> {
        let states = self.storage.list_download_states()?;
        Ok(states.into_iter().filter(|s| !s.is_complete()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_manager_creation() {
        let manager = DownloadManager::new(1024 * 1024);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_range_header_format() {
        // Test Range header formatting
        let start = 100u64;
        let end = 199u64;
        let header = format!("bytes={}-{}", start, end);
        assert_eq!(header, "bytes=100-199");

        // Test single byte range
        let header = format!("bytes={}-{}", start, start);
        assert_eq!(header, "bytes=100-100");

        // Test open-ended range (not used in our implementation but valid)
        let header = format!("bytes={}-", start);
        assert_eq!(header, "bytes=100-");
    }
}
