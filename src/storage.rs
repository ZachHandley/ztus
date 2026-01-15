//! State persistence for ztus
//!
//! This module handles storing and loading upload state for resume capability.

use crate::error::{Result, ZtusError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Upload state persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadState {
    /// Unique identifier for this upload
    pub id: String,

    /// Original file path
    pub file_path: PathBuf,

    /// Upload URL
    pub upload_url: String,

    /// Total file size
    pub file_size: u64,

    /// Current upload offset
    pub offset: u64,

    /// Timestamp when upload was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Last update timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl UploadState {
    /// Create a new upload state
    pub fn new(file_path: PathBuf, upload_url: String, file_size: u64) -> Self {
        let now = chrono::Utc::now();
        let id = Self::generate_id(&file_path, &upload_url);

        Self {
            id,
            file_path,
            upload_url,
            file_size,
            offset: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Generate a unique ID for an upload
    fn generate_id(file_path: &Path, upload_url: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        file_path.hash(&mut hasher);
        upload_url.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Update the offset
    #[allow(dead_code)]
    pub fn update_offset(&mut self, new_offset: u64) {
        self.offset = new_offset;
        self.updated_at = chrono::Utc::now();
    }

    /// Check if upload is complete
    pub fn is_complete(&self) -> bool {
        self.offset >= self.file_size
    }

    /// Calculate completion percentage
    pub fn progress(&self) -> f64 {
        if self.file_size == 0 {
            100.0
        } else {
            (self.offset as f64 / self.file_size as f64) * 100.0
        }
    }
}

impl std::fmt::Display for UploadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} -> {} ({:.1}%)",
            self.file_path.display(),
            self.upload_url,
            self.progress()
        )
    }
}

/// Download state persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    /// Unique identifier for this download
    pub id: String,

    /// URL being downloaded
    pub url: String,

    /// Output file path
    pub output_path: PathBuf,

    /// Total file size
    pub total_size: u64,

    /// Current download offset
    pub offset: u64,

    /// Download configuration
    pub chunk_size: usize,

    /// Timestamp when download was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Last update timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl DownloadState {
    /// Create a new download state
    pub fn new(url: String, output_path: PathBuf, total_size: u64, chunk_size: usize) -> Self {
        let now = chrono::Utc::now();
        let id = Self::generate_id(&url, &output_path);

        Self {
            id,
            url,
            output_path,
            total_size,
            offset: 0,
            chunk_size,
            created_at: now,
            updated_at: now,
        }
    }

    /// Generate a unique ID for a download
    fn generate_id(url: &str, output_path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        output_path.hash(&mut hasher);
        format!("download_{:x}", hasher.finish())
    }

    /// Update the offset
    #[allow(dead_code)]
    pub fn update_offset(&mut self, new_offset: u64) {
        self.offset = new_offset;
        self.updated_at = chrono::Utc::now();
    }

    /// Check if download is complete
    pub fn is_complete(&self) -> bool {
        self.offset >= self.total_size
    }

    /// Calculate completion percentage
    pub fn progress(&self) -> f64 {
        if self.total_size == 0 {
            100.0
        } else {
            (self.offset as f64 / self.total_size as f64) * 100.0
        }
    }
}

impl std::fmt::Display for DownloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} -> {} ({:.1}%)",
            self.url,
            self.output_path.display(),
            self.progress()
        )
    }
}

/// State storage manager
pub struct StateStorage {
    state_dir: PathBuf,
    download_dir: PathBuf,
}

impl StateStorage {
    /// Create a new state storage manager
    pub fn new(state_dir: PathBuf) -> Result<Self> {
        // Ensure state directories exist
        let upload_dir = state_dir.join("uploads");
        let download_dir = state_dir.join("downloads");

        std::fs::create_dir_all(&upload_dir).map_err(ZtusError::from)?;
        std::fs::create_dir_all(&download_dir).map_err(ZtusError::from)?;

        Ok(Self {
            state_dir,
            download_dir,
        })
    }

    /// Get the state file path for an upload
    fn state_file_path(&self, id: &str) -> PathBuf {
        self.state_dir.join("uploads").join(format!("{}.json", id))
    }

    /// Get the state file path for a download
    fn download_state_file_path(&self, id: &str) -> PathBuf {
        self.download_dir.join(format!("{}.json", id))
    }

    /// Save upload state to disk
    pub fn save_state(&self, state: &UploadState) -> Result<()> {
        let path = self.state_file_path(&state.id);

        let json = serde_json::to_string_pretty(state).map_err(ZtusError::from)?;

        std::fs::write(&path, json).map_err(ZtusError::from)?;

        tracing::debug!("Saved upload state to: {}", path.display());

        Ok(())
    }

    /// Load upload state from disk
    pub fn load_state(&self, id: &str) -> Result<UploadState> {
        let path = self.state_file_path(id);

        if !path.exists() {
            return Err(ZtusError::InvalidState(format!(
                "State file not found: {}",
                path.display()
            )));
        }

        let json = std::fs::read_to_string(&path).map_err(ZtusError::from)?;

        let state: UploadState = serde_json::from_str(&json).map_err(ZtusError::from)?;

        tracing::debug!("Loaded upload state from: {}", path.display());

        Ok(state)
    }

    /// Delete upload state from disk
    pub fn delete_state(&self, id: &str) -> Result<()> {
        let path = self.state_file_path(id);

        if path.exists() {
            std::fs::remove_file(&path).map_err(ZtusError::from)?;
            tracing::debug!("Deleted upload state: {}", path.display());
        }

        Ok(())
    }

    /// List all upload states
    pub fn list_states(&self) -> Result<Vec<UploadState>> {
        let mut states = Vec::new();

        let upload_dir = self.state_dir.join("uploads");

        if !upload_dir.exists() {
            return Ok(states);
        }

        let entries = std::fs::read_dir(&upload_dir).map_err(ZtusError::from)?;

        for entry in entries {
            let entry = entry.map_err(ZtusError::from)?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(state) = self.load_state(
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default(),
                ) {
                    states.push(state);
                }
            }
        }

        states.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(states)
    }

    /// Find state by file path and upload URL
    #[allow(dead_code)]
    pub fn find_state(&self, file_path: &Path, upload_url: &str) -> Result<Option<UploadState>> {
        let states = self.list_states()?;

        let id = UploadState::generate_id(file_path, upload_url);

        Ok(states.into_iter().find(|s| s.id == id))
    }

    /// Clean up old incomplete states (older than 7 days)
    pub fn cleanup_old_states(&self, days: i64) -> Result<usize> {
        let states = self.list_states()?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let mut cleaned = 0;

        for state in states {
            if !state.is_complete() && state.updated_at < cutoff {
                self.delete_state(&state.id)?;
                cleaned += 1;
            }
        }

        if cleaned > 0 {
            tracing::info!("Cleaned up {} old upload states", cleaned);
        }

        Ok(cleaned)
    }

    /// Save download state to disk
    pub fn save_download_state(&self, state: &DownloadState) -> Result<()> {
        let path = self.download_state_file_path(&state.id);

        let json = serde_json::to_string_pretty(state).map_err(ZtusError::from)?;

        std::fs::write(&path, json).map_err(ZtusError::from)?;

        tracing::debug!("Saved download state to: {}", path.display());

        Ok(())
    }

    /// Load download state from disk
    pub fn load_download_state(&self, id: &str) -> Result<DownloadState> {
        let path = self.download_state_file_path(id);

        if !path.exists() {
            return Err(ZtusError::InvalidState(format!(
                "Download state file not found: {}",
                path.display()
            )));
        }

        let json = std::fs::read_to_string(&path).map_err(ZtusError::from)?;

        let state: DownloadState = serde_json::from_str(&json).map_err(ZtusError::from)?;

        tracing::debug!("Loaded download state from: {}", path.display());

        Ok(state)
    }

    /// Delete download state from disk
    pub fn delete_download_state(&self, id: &str) -> Result<()> {
        let path = self.download_state_file_path(id);

        if path.exists() {
            std::fs::remove_file(&path).map_err(ZtusError::from)?;
            tracing::debug!("Deleted download state: {}", path.display());
        }

        Ok(())
    }

    /// List all download states
    pub fn list_download_states(&self) -> Result<Vec<DownloadState>> {
        let mut states = Vec::new();

        if !self.download_dir.exists() {
            return Ok(states);
        }

        let entries = std::fs::read_dir(&self.download_dir).map_err(ZtusError::from)?;

        for entry in entries {
            let entry = entry.map_err(ZtusError::from)?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(state) = self.load_download_state(
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default(),
                ) {
                    states.push(state);
                }
            }
        }

        states.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(states)
    }

    /// Clean up old incomplete download states
    #[allow(dead_code)]
    pub fn cleanup_old_download_states(&self, days: i64) -> Result<usize> {
        let states = self.list_download_states()?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let mut cleaned = 0;

        for state in states {
            if !state.is_complete() && state.updated_at < cutoff {
                self.delete_download_state(&state.id)?;
                cleaned += 1;
            }
        }

        if cleaned > 0 {
            tracing::info!("Cleaned up {} old download states", cleaned);
        }

        Ok(cleaned)
    }
}
