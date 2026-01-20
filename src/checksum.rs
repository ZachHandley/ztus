//! Checksum calculation utilities
//!
//! This module provides checksum calculation functionality for file uploads.

use crate::config::ChecksumAlgorithm;
use crate::error::{Result, ZtusError};
use std::io::Read;
use std::path::Path;

/// Calculate checksum for a file
pub fn calculate_file_checksum(path: &Path, algorithm: ChecksumAlgorithm) -> Result<String> {
    let mut file = std::fs::File::open(path)
        .map_err(ZtusError::IoError)?;

    match algorithm {
        ChecksumAlgorithm::Sha1 => {
            use sha1::Digest;
            let mut hasher = sha1::Sha1::new();
            let mut buffer = vec![0u8; 8192];

            loop {
                let n = file.read(&mut buffer)
                    .map_err(ZtusError::from)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }

            Ok(format!("{:x}", hasher.finalize()))
        }
        ChecksumAlgorithm::Sha256 => {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            let mut buffer = vec![0u8; 8192];

            loop {
                let n = file.read(&mut buffer)
                    .map_err(ZtusError::from)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }

            Ok(format!("{:x}", hasher.finalize()))
        }
    }
}

/// Calculate checksum for a byte slice
#[allow(dead_code)]
pub fn calculate_checksum(data: &[u8], algorithm: ChecksumAlgorithm) -> String {
    match algorithm {
        ChecksumAlgorithm::Sha1 => {
            use sha1::Digest;
            let mut hasher = sha1::Sha1::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        }
        ChecksumAlgorithm::Sha256 => {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_checksum_sha1() {
        let data = b"Hello, World!";
        let checksum = calculate_checksum(data, ChecksumAlgorithm::Sha1);
        // Known SHA1 hash for "Hello, World!"
        assert_eq!(checksum, "0a0a9f2a6772942557ab5355d76af442f8f65e01");
    }

    #[test]
    fn test_calculate_checksum_sha256() {
        let data = b"Hello, World!";
        let checksum = calculate_checksum(data, ChecksumAlgorithm::Sha256);
        // Known SHA256 hash for "Hello, World!"
        assert_eq!(checksum, "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f");
    }
}
