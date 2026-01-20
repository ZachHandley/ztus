//! Upload state machine and manager
//!
//! This module handles the file upload process with resume capability.

use crate::checksum::calculate_file_checksum;
use crate::config::UploadConfig;
use crate::error::{Result, ZtusError};
use crate::progress::{ProgressReporter, TerminalProgress};
use crate::protocol::TusProtocol;
use crate::storage::{StateStorage, UploadState};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Upload manager
pub struct UploadManager {
    protocol: TusProtocol,
    config: UploadConfig,
    storage: StateStorage,
    progress: Box<dyn ProgressReporter>,
}

impl UploadManager {
    async fn read_chunk(file: &mut File, buffer: &mut [u8]) -> Result<usize> {
        let mut filled = 0;
        while filled < buffer.len() {
            let n = file
                .read(&mut buffer[filled..])
                .await
                .map_err(ZtusError::from)?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        Ok(filled)
    }

    /// Create a new upload manager
    pub fn new(base_url: String, config: UploadConfig, state_dir: PathBuf) -> Result<Self> {
        let headers = config.headers.clone();
        let protocol = if headers.is_empty() {
            TusProtocol::new(
                base_url,
                Duration::from_secs(config.timeout),
            )?
        } else {
            TusProtocol::with_headers(
                base_url,
                Duration::from_secs(config.timeout),
                headers,
            )?
        };

        let storage = StateStorage::new(state_dir)?;

        Ok(Self {
            protocol,
            config,
            storage,
            progress: Box::new(TerminalProgress::new(0)),
        })
    }

    /// Create a new upload manager with a custom progress reporter
    pub fn with_progress(
        base_url: String,
        config: UploadConfig,
        state_dir: PathBuf,
        progress: Box<dyn ProgressReporter>,
    ) -> Result<Self> {
        let headers = config.headers.clone();
        let protocol = if headers.is_empty() {
            TusProtocol::new(
                base_url,
                Duration::from_secs(config.timeout),
            )?
        } else {
            TusProtocol::with_headers(
                base_url,
                Duration::from_secs(config.timeout),
                headers,
            )?
        };

        let storage = StateStorage::new(state_dir)?;

        Ok(Self {
            protocol,
            config,
            storage,
            progress,
        })
    }

    /// Upload a file with resume support
    pub async fn upload_file(&self, file_path: &Path) -> Result<()> {
        // Validate file exists
        if !file_path.exists() {
            return Err(ZtusError::FileNotFound(file_path.display().to_string()));
        }

        // Get file metadata
        let file_size = std::fs::metadata(file_path)
            .map_err(ZtusError::from)?
            .len();

        let file_path_buf = file_path.to_path_buf();

        // Check for existing upload state
        let states = self.storage.list_states()?;
        let existing_state = states
            .into_iter()
            .find(|s| s.file_path == file_path_buf);

        let existing_state = if !self.config.resume {
            if let Some(state) = existing_state {
                tracing::info!("Resume disabled; deleting existing state and starting new upload");
                self.storage.delete_state(&state.id)?;
            }
            None
        } else {
            existing_state
        };

        let (upload_url, mut offset, state_id) = match existing_state {
            Some(state) => {
                // Verify file hasn't changed
                if state.file_size != file_size {
                    tracing::info!("File size differs from saved state, starting new upload");
                    self.start_new_upload(&file_path_buf, file_size).await?
                } else {
                    tracing::info!("Resuming existing upload from offset: {}", state.offset);
                    // Verify server state
                    match self.protocol.get_upload_offset(&state.upload_url).await {
                        Ok(server_offset) => {
                            if server_offset != state.offset {
                                tracing::warn!(
                                    "Offset mismatch: local={}, server={}, using server offset",
                                    state.offset,
                                    server_offset
                                );
                                (state.upload_url, server_offset, state.id)
                            } else {
                                (state.upload_url, state.offset, state.id)
                            }
                        }
                        Err(ZtusError::UploadTerminated) => {
                            tracing::warn!("Upload terminated by server, starting new upload");
                            self.start_new_upload(&file_path_buf, file_size).await?
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
            None => self.start_new_upload(&file_path_buf, file_size).await?,
        };

        // Open file for reading
        let mut file = File::open(&file_path_buf).await.map_err(ZtusError::from)?;

        // Seek to current offset
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(ZtusError::from)?;
        }

        // Initialize progress bar with total bytes
        self.progress.set_total(file_size);
        self.progress.report_progress(offset, file_size)?;

        // Log chunk size being used
        tracing::info!("Chunk size: {} MB", self.config.chunk_size / 1024 / 1024);
        tracing::info!("Starting upload from offset: {} / {} bytes", offset, file_size);

        // Initialize adaptive chunk sizing
        let (mut current_chunk_size, mut throughput_history) = if self.config.adaptive.enabled {
            tracing::info!("Adaptive chunk sizing enabled");
            tracing::info!("Initial chunk size: {} MB", self.config.adaptive.initial_chunk_size / 1024 / 1024);
            tracing::info!("Chunk size range: {} MB - {} MB",
                self.config.adaptive.min_chunk_size / 1024 / 1024,
                self.config.adaptive.max_chunk_size / 1024 / 1024
            );
            (self.config.adaptive.initial_chunk_size, Vec::with_capacity(5))
        } else {
            (self.config.chunk_size, Vec::new())
        };

        // Upload in chunks
        let mut buffer = vec![0u8; current_chunk_size];
        let mut chunk_num = offset / current_chunk_size as u64;
        let mut last_percent = if file_size > 0 {
            (offset * 100) / file_size
        } else {
            100
        };
        let mut last_saved_percent = last_percent;  // Track last saved percentage for state saves

        loop {
            // Reallocate buffer if chunk size changed (adaptive mode)
            if current_chunk_size != buffer.len() {
                buffer = vec![0u8; current_chunk_size];
            }

            // Read chunk
            let n = Self::read_chunk(&mut file, &mut buffer).await?;

            if n == 0 {
                break; // EOF
            }

            let chunk = buffer[..n].to_vec();

            // Verbose logging for each chunk
            if self.config.verbose {
                tracing::debug!(
                    "Uploading chunk #{}: {} bytes (offset: {})",
                    chunk_num,
                    n,
                    offset
                );
            }
            if self.config.verbose
                && n < current_chunk_size
                && offset + (n as u64) < file_size
            {
                tracing::debug!(
                    "Short read before EOF: {} bytes (expected {} bytes)",
                    n,
                    current_chunk_size
                );
            }

            // Calculate current percentage before uploading
            let current_percent = if file_size > 0 {
                (offset * 100) / file_size
            } else {
                100
            };

            // Save state before uploading, but only at percentage intervals to avoid blocking
            // Save on: first upload (0%), at configured intervals, and before completion
            let should_save_state = offset == 0  // Always save initial state
                || current_percent >= last_saved_percent + self.config.state_save_interval as u64  // Every X%
                || offset + n as u64 == file_size;  // Always save before completion

            if should_save_state {
                let state = UploadState {
                    id: state_id.clone(),
                    file_path: file_path_buf.clone(),
                    upload_url: upload_url.clone(),
                    file_size,
                    offset,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                match self.storage.save_state(&state) {
                    Ok(()) => {
                        last_saved_percent = current_percent;
                        tracing::debug!("Saved state at {}% (offset: {})", current_percent, offset);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to save state before chunk: {}", e);
                    }
                }
            }

            // Upload chunk with retry logic and timing (for adaptive sizing)
            let start_time = if self.config.adaptive.enabled {
                Some(Instant::now())
            } else {
                None
            };

            let new_offset = match self
                .protocol
                .upload_chunk_with_retry(
                    &upload_url,
                    offset,
                    chunk,
                    self.config.max_retries,
                )
                .await
            {
                Ok(new_offset) => new_offset,
                Err(e) => {
                    // Save state before returning error (best-effort)
                    let error_state = UploadState {
                        id: state_id.clone(),
                        file_path: file_path_buf.clone(),
                        upload_url: upload_url.clone(),
                        file_size,
                        offset,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };
                    let _ = self.storage.save_state(&error_state);
                    return Err(e);
                }
            };

            let (bytes_sent, duration_secs) = if let Some(start) = start_time {
                let duration = start.elapsed();
                (n, duration.as_secs_f64())
            } else {
                (0, 0.0)
            };

            if self.config.verbose {
                tracing::debug!("Chunk #{} complete, new offset: {}", chunk_num, new_offset);
            }
            chunk_num += 1;

            // Adaptive chunk sizing logic
            if self.config.adaptive.enabled && duration_secs > 0.0 {
                const MIN_EXPLORATION_THROUGHPUT: f64 = 10.0;
                // Calculate throughput for this chunk
                let throughput_mibps = (bytes_sent as f64 / 1024.0 / 1024.0) / duration_secs;

                // Add to history (keep last 5)
                throughput_history.push(throughput_mibps);
                if throughput_history.len() > 5 {
                    throughput_history.remove(0);
                }

                // Check if we should adapt (every adaptation_interval chunks)
                if chunk_num % self.config.adaptive.adaptation_interval as u64 == 0
                    && throughput_history.len() >= 2 {

                    let avg_throughput: f64 = throughput_history.iter().sum::<f64>() / throughput_history.len() as f64;

                    // Calculate variance from average
                    let variance: f64 = throughput_history.iter()
                        .map(|&t| (t - avg_throughput).abs() / avg_throughput)
                        .sum::<f64>() / throughput_history.len() as f64;

                    let old_chunk_size = current_chunk_size;

                    // Adapt chunk size based on throughput trend
                    if variance > self.config.adaptive.stability_threshold {
                        // Unstable - check trend
                        let first_half_avg: f64 = throughput_history.iter()
                            .take(throughput_history.len() / 2)
                            .sum::<f64>() / (throughput_history.len() / 2) as f64;
                        let second_half_avg: f64 = throughput_history.iter()
                            .skip(throughput_history.len() / 2)
                            .sum::<f64>() / (throughput_history.len() - throughput_history.len() / 2) as f64;

                        let throughput_change = (second_half_avg - first_half_avg) / first_half_avg;

                        if throughput_change > self.config.adaptive.stability_threshold {
                            // Throughput increasing - double chunk size
                            current_chunk_size = (current_chunk_size * 2)
                                .min(self.config.adaptive.max_chunk_size);
                            if current_chunk_size != old_chunk_size {
                                tracing::info!(
                                    "Adaptive: Chunk size {} MB → {} MB (throughput: {:.1} MiB/s, increasing)",
                                    old_chunk_size / 1024 / 1024,
                                    current_chunk_size / 1024 / 1024,
                                    avg_throughput
                                );
                            }
                        } else if throughput_change < -self.config.adaptive.stability_threshold {
                            // Throughput decreasing - halve chunk size
                            current_chunk_size = (current_chunk_size / 2)
                                .max(self.config.adaptive.min_chunk_size);
                            if current_chunk_size != old_chunk_size {
                                tracing::info!(
                                    "Adaptive: Chunk size {} MB → {} MB (throughput: {:.1} MiB/s, decreasing)",
                                    old_chunk_size / 1024 / 1024,
                                    current_chunk_size / 1024 / 1024,
                                    avg_throughput
                                );
                            }
                        }
                    } else {
                        // Stable throughput - but check if it's too slow
                        if avg_throughput < MIN_EXPLORATION_THROUGHPUT && current_chunk_size < self.config.adaptive.max_chunk_size {
                            // Stable but slow - proactively try larger chunk size
                            let new_size = (current_chunk_size * 2).min(self.config.adaptive.max_chunk_size);
                            tracing::info!(
                                "Adaptive: Chunk size {} MB → {} MB (throughput: {:.1} MiB/s, stable but slow - exploring larger chunks)",
                                current_chunk_size / 1024 / 1024,
                                new_size / 1024 / 1024,
                                avg_throughput
                            );
                            current_chunk_size = new_size;
                        } else {
                            // Stable and acceptable throughput - maintain current size
                            tracing::debug!(
                                "Adaptive: Chunk size stable at {} MB (throughput: {:.1} MiB/s, variance: {:.1}%)",
                                current_chunk_size / 1024 / 1024,
                                avg_throughput,
                                variance * 100.0
                            );
                        }
                    }
                }
            }

            // Validate offset
            if new_offset != offset + n as u64 {
                // Offset mismatch - query server and retry
                tracing::warn!(
                    "Offset mismatch after chunk: expected={}, got={}",
                    offset + n as u64,
                    new_offset
                );

                let server_offset = self.protocol.get_upload_offset(&upload_url).await?;

                // Seek to correct position
                file.seek(std::io::SeekFrom::Start(server_offset))
                    .await
                    .map_err(ZtusError::from)?;

                offset = server_offset;
                self.progress.report_progress(offset, file_size)?;

                // Update state
                let state = UploadState {
                    id: state_id.clone(),
                    file_path: file_path_buf.clone(),
                    upload_url: upload_url.clone(),
                    file_size,
                    offset,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                self.storage.save_state(&state)?;

                continue;
            }

            offset = new_offset;
            self.progress.report_progress(offset, file_size)?;
            if file_size > 0 {
                let percent = (offset * 100) / file_size;
                if percent > last_percent {
                    last_percent = percent;
                    tracing::info!(
                        "Progress: {}% ({} / {} bytes)",
                        percent,
                        offset,
                        file_size
                    );
                }
            }

            // Calculate new percentage after upload
            let new_percent = if file_size > 0 {
                (offset * 100) / file_size
            } else {
                100
            };

            // Update state (but only at percentage intervals to reduce I/O)
            if new_percent >= last_saved_percent + self.config.state_save_interval as u64
                || offset == file_size {
                let state = UploadState {
                    id: state_id.clone(),
                    file_path: file_path_buf.clone(),
                    upload_url: upload_url.clone(),
                    file_size,
                    offset,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                match self.storage.save_state(&state) {
                    Ok(()) => {
                        last_saved_percent = new_percent;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to save state after chunk: {}", e);
                    }
                }
            }
        }

        self.progress.finish("Upload complete!");

        // Clean up state file
        self.storage.delete_state(&state_id)?;

        tracing::info!("File uploaded successfully: {}", file_path.display());

        Ok(())
    }

    /// Terminate an upload at the given URL
    #[allow(dead_code)]
    pub async fn terminate_upload(&self, upload_url: &str) -> Result<()> {
        self.protocol.terminate_upload(upload_url).await
    }

    /// Terminate an upload and clean up local state
    #[allow(dead_code)]
    pub async fn terminate_and_cleanup(&self, upload_url: &str, state_id: &str) -> Result<()> {
        self.protocol.terminate_upload(upload_url).await?;
        self.storage.delete_state(state_id)?;
        tracing::info!("Upload terminated and cleaned up: {}", upload_url);
        Ok(())
    }

    /// Start a new upload
    async fn start_new_upload(
        &self,
        file_path: &Path,
        file_size: u64,
    ) -> Result<(String, u64, String)> {
        // Check server capabilities before starting upload
        let capabilities = self.protocol.discover_capabilities().await;

        if let Ok(caps) = &capabilities {
            // Check if server supports creation extension
            if !caps.extensions.contains(&crate::protocol::TusExtension::Creation) {
                tracing::warn!(
                    "Server does not support the 'creation' extension. Upload may fail."
                );
            }

            // Check if server supports checksum extension when verification is enabled
            if self.config.verify_checksum
                && !caps.extensions.contains(&crate::protocol::TusExtension::Checksum)
            {
                tracing::warn!(
                    "Checksum verification is enabled but server does not support 'checksum' extension. Disabling checksum verification."
                );
            }

            // Check max file size
            if let Some(max_size) = caps.max_size {
                if file_size > max_size {
                    return Err(ZtusError::ProtocolError(format!(
                        "File size ({} bytes) exceeds server maximum ({} bytes)",
                        file_size, max_size
                    )));
                }
            }
        }

        // Calculate checksum if verification is enabled
        let (checksum_algo, checksum_value) = if self.config.verify_checksum {
            // Re-check capabilities after discovering them
            let supports_checksum = capabilities
                .as_ref()
                .map(|c| c.extensions.contains(&crate::protocol::TusExtension::Checksum))
                .unwrap_or(true); // Assume supported if we couldn't check

            if supports_checksum {
                let checksum = calculate_file_checksum(file_path, self.config.checksum_algorithm)?;
                tracing::info!(
                    "Calculated {} checksum: {}",
                    self.config.checksum_algorithm.as_tus_algorithm(),
                    checksum
                );
                (Some(self.config.checksum_algorithm), Some(checksum))
            } else {
                tracing::info!("Skipping checksum calculation (not supported by server)");
                (None, None)
            }
        } else {
            (None, None)
        };

        // Try to create upload, with fallback for unsupported checksum algorithms
        let upload_url = match self
            .protocol
            .create_upload(
                file_size,
                Some(self.config.metadata.clone()),
                checksum_algo,
                checksum_value.clone(),
            )
            .await
        {
            Ok(url) => url,
            Err(ZtusError::ProtocolError(ref error_msg))
                if checksum_algo.is_some() &&
                   (error_msg.contains("unsupported checksum algorithm") ||
                    error_msg.contains("Unsupported checksum algorithm") ||
                    error_msg.contains("checksum algorithm") ||
                    error_msg.contains("checksum not supported")) =>
            {
                // Server doesn't support the configured checksum algorithm
                if checksum_algo == Some(crate::config::ChecksumAlgorithm::Sha1) {
                    // Try SHA256 as fallback
                    tracing::warn!(
                        "Server does not support SHA1 checksums, falling back to SHA256"
                    );
                    let sha256_checksum = calculate_file_checksum(file_path, crate::config::ChecksumAlgorithm::Sha256)?;
                    tracing::info!(
                        "Calculated sha256 checksum: {}",
                        sha256_checksum
                    );

                    self.protocol
                        .create_upload(
                            file_size,
                            Some(self.config.metadata.clone()),
                            Some(crate::config::ChecksumAlgorithm::Sha256),
                            Some(sha256_checksum),
                        )
                        .await?
                } else {
                    // Try without checksum verification as last resort
                    tracing::warn!(
                        "Server does not support the requested checksum algorithm, \
                         retrying without checksum verification"
                    );
                    self.protocol
                        .create_upload(
                            file_size,
                            Some(self.config.metadata.clone()),
                            None,
                            None,
                        )
                        .await?
                }
            }
            Err(e) => return Err(e),
        };

        tracing::info!("Created upload at: {}", upload_url);

        // Initialize upload state
        let state = UploadState::new(
            file_path.to_path_buf(),
            upload_url.clone(),
            file_size,
        );

        let state_id = state.id.clone();

        // Save initial state
        self.storage.save_state(&state)?;

        Ok((upload_url, 0, state_id))
    }

    /// Resume an incomplete upload
    pub async fn resume_upload(&self, file_path: &Path) -> Result<()> {
        // Check for existing upload state
        let file_path_buf = file_path.to_path_buf();
        let states = self.storage.list_states()?;
        let existing_state = states.into_iter().find(|s| s.file_path == file_path_buf);

        match existing_state {
            Some(state) => {
                tracing::info!(
                    "Resuming upload for: {} (offset: {} / {})",
                    file_path.display(),
                    state.offset,
                    state.file_size
                );

                if state.is_complete() {
                    tracing::info!("Upload already complete");
                    return Ok(());
                }

                // Delegate to upload_file which handles resume logic
                self.upload_file(file_path).await
            }
            None => {
                tracing::warn!("No existing upload state found, starting new upload");
                self.upload_file(file_path).await
            }
        }
    }

    /// List all incomplete uploads
    pub fn list_incomplete(&self) -> Result<Vec<UploadState>> {
        let states = self.storage.list_states()?;
        Ok(states.into_iter().filter(|s| !s.is_complete()).collect())
    }

    /// Clean up old incomplete uploads
    pub fn cleanup(&self, days: i64) -> Result<usize> {
        self.storage.cleanup_old_states(days)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let config = UploadConfig::new();
        assert!(config.validate().is_ok());

        let invalid_config = UploadConfig {
            chunk_size: 0,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());
    }
}
