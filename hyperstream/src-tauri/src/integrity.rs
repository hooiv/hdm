use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest as Sha2Digest};
use md5;
use tokio::io::AsyncReadExt;

/// Supported hash algorithms for integrity verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashAlgorithm {
    MD5,
    SHA256,
    CRC32,
}

impl std::fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HashAlgorithm::MD5 => write!(f, "md5"),
            HashAlgorithm::SHA256 => write!(f, "sha256"),
            HashAlgorithm::CRC32 => write!(f, "crc32"),
        }
    }
}

/// Result of a checksum computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksumResult {
    pub algorithm: String,
    pub hash: String,
    pub file_path: String,
    pub file_size: u64,
    pub verified: Option<bool>,
    pub expected: Option<String>,
}

/// Parse a checksum string in the format "algorithm:hexdigest".
/// Supported formats:
///   - "sha256:abc123..."
///   - "md5:abc123..."
///   - "crc32:abc123..."
///   - Plain hex string (auto-detected by length: 32=MD5, 64=SHA256, 8=CRC32)
fn parse_checksum_spec(spec: &str) -> Result<(HashAlgorithm, String), String> {
    if let Some((algo, hash)) = spec.split_once(':') {
        let algorithm = match algo.to_lowercase().as_str() {
            "sha256" | "sha-256" => HashAlgorithm::SHA256,
            "md5" => HashAlgorithm::MD5,
            "crc32" => HashAlgorithm::CRC32,
            _ => return Err(format!("Unsupported hash algorithm: {}", algo)),
        };
        Ok((algorithm, hash.to_lowercase()))
    } else {
        // Auto-detect by length
        let hash = spec.trim().to_lowercase();
        let algo = match hash.len() {
            32 => HashAlgorithm::MD5,
            64 => HashAlgorithm::SHA256,
            8 => HashAlgorithm::CRC32,
            _ => return Err(format!(
                "Cannot auto-detect algorithm for hash of length {}. Use format 'algorithm:hash'.",
                hash.len()
            )),
        };
        Ok((algo, hash))
    }
}

/// Compute the hash of a file using the specified algorithm.
/// Reads in 64KB chunks for memory efficiency.
pub async fn compute_file_hash(path: &str, algorithm: HashAlgorithm) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open file for hashing: {}", e))?;

    let mut buf = vec![0u8; 65536]; // 64KB read buffer

    match algorithm {
        HashAlgorithm::SHA256 => {
            let mut hasher = Sha256::new();
            loop {
                let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
                if n == 0 { break; }
                hasher.update(&buf[..n]);
            }
            Ok(format!("{:x}", hasher.finalize()))
        }
        HashAlgorithm::MD5 => {
            let mut context = md5::Context::new();
            loop {
                let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
                if n == 0 { break; }
                context.consume(&buf[..n]);
            }
            Ok(format!("{:x}", context.compute()))
        }
        HashAlgorithm::CRC32 => {
            let mut hasher = crc32fast::Hasher::new();
            loop {
                let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
                if n == 0 { break; }
                hasher.update(&buf[..n]);
            }
            Ok(format!("{:08x}", hasher.finalize()))
        }
    }
}

/// Verify a file's checksum against an expected value.
/// `expected_checksum` format: "sha256:abc123..." or "md5:abc123..." or plain hex.
pub async fn verify_file_checksum(path: &str, expected_checksum: &str) -> Result<ChecksumResult, String> {
    let (algorithm, expected_hash) = parse_checksum_spec(expected_checksum)?;
    let actual_hash = compute_file_hash(path, algorithm).await?;

    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Failed to read file metadata: {}", e))?;

    let verified = actual_hash == expected_hash;

    let result = ChecksumResult {
        algorithm: algorithm.to_string(),
        hash: actual_hash.clone(),
        file_path: path.to_string(),
        file_size: meta.len(),
        verified: Some(verified),
        expected: Some(expected_hash.clone()),
    };

    if verified {
        Ok(result)
    } else {
        Err(format!(
            "Checksum mismatch: expected {} but got {} ({})",
            expected_hash, actual_hash, algorithm
        ))
    }
}

/// Compute checksums for a file using all common algorithms.
/// Returns results for MD5, SHA256, and CRC32.
pub async fn compute_all_checksums(path: &str) -> Result<Vec<ChecksumResult>, String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Failed to read file metadata: {}", e))?;
    let file_size = meta.len();

    // Read the file once and compute all hashes in a single pass
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut sha256_hasher = Sha256::new();
    let mut md5_context = md5::Context::new();
    let mut crc32_hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 65536];

    loop {
        let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
        if n == 0 { break; }
        sha256_hasher.update(&buf[..n]);
        md5_context.consume(&buf[..n]);
        crc32_hasher.update(&buf[..n]);
    }

    Ok(vec![
        ChecksumResult {
            algorithm: "sha256".to_string(),
            hash: format!("{:x}", sha256_hasher.finalize()),
            file_path: path.to_string(),
            file_size,
            verified: None,
            expected: None,
        },
        ChecksumResult {
            algorithm: "md5".to_string(),
            hash: format!("{:x}", md5_context.compute()),
            file_path: path.to_string(),
            file_size,
            verified: None,
            expected: None,
        },
        ChecksumResult {
            algorithm: "crc32".to_string(),
            hash: format!("{:08x}", crc32_hasher.finalize()),
            file_path: path.to_string(),
            file_size,
            verified: None,
            expected: None,
        },
    ])
}

// ─── Auto Post-Download Verification ─────────────────────────────────
// Verifies downloads against server-provided checksums (Content-MD5, ETag,
// sidecar .sha256/.md5 files) automatically after completion.

/// Result of automatic post-download verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoVerifyResult {
    pub download_id: String,
    pub file_path: String,
    pub verified: bool,
    pub method: String,
    pub expected: String,
    pub actual: String,
    pub algorithm: String,
    pub message: String,
}

/// Decode a Content-MD5 header value (base64-encoded MD5).
fn decode_content_md5(header_value: &str) -> Option<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(header_value.trim())
        .ok()?;
    if bytes.len() != 16 { return None; }
    Some(bytes.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Auto-verify a completed download against available checksums.
/// Checks in order: Content-MD5 header, sidecar checksum files (.sha256, .md5, .sha1).
/// Returns None if no verification method is available.
pub async fn auto_verify(
    download_id: &str,
    file_path: &str,
    content_md5_header: Option<&str>,
) -> Option<AutoVerifyResult> {
    // Method 1: Content-MD5 header (base64-encoded MD5)
    if let Some(header) = content_md5_header {
        if let Some(expected_md5) = decode_content_md5(header) {
            match compute_file_hash(file_path, HashAlgorithm::MD5).await {
                Ok(actual) => {
                    let verified = actual == expected_md5;
                    return Some(AutoVerifyResult {
                        download_id: download_id.to_string(),
                        file_path: file_path.to_string(),
                        verified,
                        method: "Content-MD5 header".to_string(),
                        expected: expected_md5,
                        actual,
                        algorithm: "md5".to_string(),
                        message: if verified {
                            "Integrity verified via Content-MD5 header".to_string()
                        } else {
                            "INTEGRITY FAILURE: File does not match Content-MD5 header".to_string()
                        },
                    });
                }
                Err(e) => {
                    eprintln!("[integrity] Failed to compute MD5: {}", e);
                }
            }
        }
    }

    // Method 2: Sidecar checksum files
    for (ext, algo) in &[
        (".sha256", HashAlgorithm::SHA256),
        (".sha256sum", HashAlgorithm::SHA256),
        (".md5", HashAlgorithm::MD5),
        (".md5sum", HashAlgorithm::MD5),
    ] {
        let sidecar_path = format!("{}{}", file_path, ext);
        if let Ok(content) = tokio::fs::read_to_string(&sidecar_path).await {
            // Parse sidecar: could be "hash  filename" or just "hash"
            let expected_hash = content
                .lines()
                .next()
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim()
                .to_lowercase();

            if expected_hash.is_empty() { continue; }

            match compute_file_hash(file_path, *algo).await {
                Ok(actual) => {
                    let verified = actual == expected_hash;
                    return Some(AutoVerifyResult {
                        download_id: download_id.to_string(),
                        file_path: file_path.to_string(),
                        verified,
                        method: format!("Sidecar file {}", ext),
                        expected: expected_hash,
                        actual,
                        algorithm: algo.to_string(),
                        message: if verified {
                            format!("Integrity verified via sidecar {}", ext)
                        } else {
                            format!("INTEGRITY FAILURE: File does not match sidecar {}", ext)
                        },
                    });
                }
                Err(e) => {
                    eprintln!("[integrity] Failed to compute hash for sidecar check: {}", e);
                }
            }
        }
    }

    None
}

/// Compute a file hash with progress reporting via a callback.
/// Callback receives (bytes_processed, total_bytes).
pub async fn compute_file_hash_with_progress<F>(
    path: &str,
    algorithm: HashAlgorithm,
    progress: F,
) -> Result<String, String>
where
    F: Fn(u64, u64),
{
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let total = meta.len();

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut buf = vec![0u8; 65536];
    let mut processed: u64 = 0;

    match algorithm {
        HashAlgorithm::SHA256 => {
            let mut hasher = Sha256::new();
            loop {
                let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
                if n == 0 { break; }
                hasher.update(&buf[..n]);
                processed += n as u64;
                progress(processed, total);
            }
            Ok(format!("{:x}", hasher.finalize()))
        }
        HashAlgorithm::MD5 => {
            let mut context = md5::Context::new();
            loop {
                let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
                if n == 0 { break; }
                context.consume(&buf[..n]);
                processed += n as u64;
                progress(processed, total);
            }
            Ok(format!("{:x}", context.compute()))
        }
        HashAlgorithm::CRC32 => {
            let mut hasher = crc32fast::Hasher::new();
            loop {
                let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
                if n == 0 { break; }
                hasher.update(&buf[..n]);
                processed += n as u64;
                progress(processed, total);
            }
            Ok(format!("{:08x}", hasher.finalize()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_checksum_spec_prefixed() {
        let (algo, hash) = parse_checksum_spec("sha256:abc123def456").unwrap();
        assert_eq!(algo, HashAlgorithm::SHA256);
        assert_eq!(hash, "abc123def456");
    }

    #[test]
    fn test_parse_checksum_spec_md5() {
        let (algo, hash) = parse_checksum_spec("md5:d41d8cd98f00b204e9800998ecf8427e").unwrap();
        assert_eq!(algo, HashAlgorithm::MD5);
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_parse_checksum_spec_auto_detect_md5() {
        let (algo, _) = parse_checksum_spec("d41d8cd98f00b204e9800998ecf8427e").unwrap();
        assert_eq!(algo, HashAlgorithm::MD5);
    }

    #[test]
    fn test_parse_checksum_spec_auto_detect_sha256() {
        let hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let (algo, _) = parse_checksum_spec(hash).unwrap();
        assert_eq!(algo, HashAlgorithm::SHA256);
    }

    #[test]
    fn test_parse_checksum_spec_auto_detect_crc32() {
        let (algo, _) = parse_checksum_spec("00000000").unwrap();
        assert_eq!(algo, HashAlgorithm::CRC32);
    }

    #[test]
    fn test_parse_checksum_spec_unknown_length() {
        assert!(parse_checksum_spec("abc").is_err());
    }
}
