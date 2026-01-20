//! Progress reporting abstraction for terminal UI and Python callbacks
//!
//! This module provides a trait-based progress reporting system that allows
//! different implementations (terminal UI, Python callbacks, etc.)

use crate::error::Result;

/// Trait for progress reporting during uploads/downloads
pub trait ProgressReporter: Send + Sync {
    /// Report progress update
    ///
    /// # Arguments
    /// * `bytes_transferred` - Total bytes transferred so far
    /// * `total_bytes` - Total bytes to transfer (0 if unknown)
    fn report_progress(&self, bytes_transferred: u64, total_bytes: u64) -> Result<()>;

    /// Set total bytes (called when total becomes known)
    fn set_total(&self, total_bytes: u64);

    /// Mark progress as complete with a message
    fn finish(&self, _message: &str) {}
}

/// Terminal-based progress reporter using indicatif
pub struct TerminalProgress {
    bar: indicatif::ProgressBar,
}

impl TerminalProgress {
    pub fn new(total_bytes: u64) -> Self {
        let bar = indicatif::ProgressBar::new(total_bytes);
        bar.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%) {bytes_per_sec} ETA: {eta}")
                .expect("Invalid progress template")
                .progress_chars("##-")
        );
        Self { bar }
    }
}

impl ProgressReporter for TerminalProgress {
    fn report_progress(&self, bytes_transferred: u64, _total_bytes: u64) -> Result<()> {
        self.bar.set_position(bytes_transferred);
        Ok(())
    }

    fn set_total(&self, total_bytes: u64) {
        self.bar.set_length(total_bytes);
    }

    fn finish(&self, message: &str) {
        self.bar.finish_with_message(message.to_string());
    }
}

/// No-op progress reporter for when progress isn't needed
pub struct NoOpProgress;

impl ProgressReporter for NoOpProgress {
    fn report_progress(&self, _bytes_transferred: u64, _total_bytes: u64) -> Result<()> {
        Ok(())
    }

    fn set_total(&self, _total_bytes: u64) {}

    fn finish(&self, _message: &str) {}
}
