use sha2::{Sha256, Digest};
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;

/// Validate that a file path is within the download directory (path traversal protection).
fn validate_notarize_path(file_path: &str) -> Result<std::path::PathBuf, String> {
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let canon = dunce::canonicalize(file_path)
        .map_err(|e| format!("Cannot resolve path: {}", e))?;
    if !canon.starts_with(&download_dir) {
        return Err("File must be within the download directory".to_string());
    }
    Ok(canon)
}

/// Stream-hash a file using SHA-256 without loading it entirely into memory.
async fn stream_sha256(path: &Path) -> Result<(sha2::digest::Output<Sha256>, u64), String> {
    let mut file = fs::File::open(path).await.map_err(|e| format!("Failed to open file: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf).await.map_err(|e| format!("Read error: {}", e))?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hasher.finalize(), total))
}

/// Compute SHA-256 hash of a file and submit it to a free RFC 3161 Timestamp Authority.
/// Saves the timestamp response token as a `.tsr` file alongside the original file.
pub async fn notarize_file(file_path: String) -> Result<serde_json::Value, String> {
    let canon = validate_notarize_path(&file_path)?;
    let path = canon.as_path();
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // 1. Compute SHA-256 via streaming (avoids loading entire file into memory)
    let (hash, file_size) = stream_sha256(path).await?;
    let hash_hex = hex::encode(&hash);

    // 2. Build a simple timestamp query
    // RFC 3161 TSQ (TimeStampReq) is ASN.1/DER encoded
    // We'll build a minimal TSQ manually:
    //   SEQUENCE {
    //     INTEGER 1 (version)
    //     SEQUENCE { (messageImprint)
    //       SEQUENCE { (hashAlgorithm - SHA-256)
    //         OID 2.16.840.1.101.3.4.2.1
    //       }
    //       OCTET STRING (hash)
    //     }
    //     BOOLEAN TRUE (certReq)
    //   }

    let sha256_oid: Vec<u8> = vec![
        0x30, 0x0d,                                         // SEQUENCE (13 bytes) - AlgorithmIdentifier
        0x06, 0x09,                                         // OID (9 bytes)
        0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, // SHA-256 OID
        0x05, 0x00,                                         // NULL
    ];

    let mut message_imprint = Vec::new();
    message_imprint.extend_from_slice(&sha256_oid);
    // OCTET STRING for hash (32 bytes)
    message_imprint.push(0x04); // OCTET STRING tag
    message_imprint.push(0x20); // 32 bytes length
    message_imprint.extend_from_slice(&hash);

    // Wrap in SEQUENCE
    let mi_len = message_imprint.len();
    let mut mi_seq = vec![0x30]; // SEQUENCE tag
    push_der_length(&mut mi_seq, mi_len);
    mi_seq.extend_from_slice(&message_imprint);

    // Build full TSQ
    let mut tsq_content = Vec::new();
    // version INTEGER 1
    tsq_content.extend_from_slice(&[0x02, 0x01, 0x01]);
    // messageImprint
    tsq_content.extend_from_slice(&mi_seq);
    // certReq BOOLEAN TRUE
    tsq_content.extend_from_slice(&[0x01, 0x01, 0xFF]);

    let mut tsq = vec![0x30]; // SEQUENCE tag
    push_der_length(&mut tsq, tsq_content.len());
    tsq.extend_from_slice(&tsq_content);

    // 3. Submit to FreeTSA.org
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client.post("https://freetsa.org/tsr")
        .header("Content-Type", "application/timestamp-query")
        .body(tsq)
        .send()
        .await
        .map_err(|e| format!("TSA request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("TSA returned status: {}", response.status()));
    }

    let tsr_bytes = response.bytes().await.map_err(|e| format!("Failed to read TSA response: {}", e))?;

    // 4. Save .tsr file (use canonical path to avoid inconsistency with verify)
    let tsr_path = format!("{}.tsr", canon.display());
    fs::write(&tsr_path, &tsr_bytes).await.map_err(|e| format!("Failed to save .tsr: {}", e))?;

    Ok(serde_json::json!({
        "hash": hash_hex,
        "algorithm": "SHA-256",
        "tsr_path": tsr_path,
        "tsa_url": "https://freetsa.org/tsr",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "file_size": file_size,
        "status": "notarized"
    }))
}

/// Verify a previously notarized file by checking if the hash still matches.
///
/// NOTE: This performs an integrity check (re-hash comparison) and a structural
/// TSR validation. Full cryptographic verification of the TSA signature would
/// require parsing ASN.1/CMS and validating the FreeTSA certificate chain,
/// which is beyond the scope of this lightweight verifier. For legal-grade
/// verification, use `openssl ts -verify`.
pub async fn verify_notarization(file_path: String) -> Result<serde_json::Value, String> {
    let canon = validate_notarize_path(&file_path)?;
    let path = canon.as_path();
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let tsr_path = format!("{}.tsr", canon.display());
    if !Path::new(&tsr_path).exists() {
        return Err("No .tsr notarization file found. File has not been notarized.".to_string());
    }

    // Compute current hash via streaming
    let (hash, _file_size) = stream_sha256(path).await?;
    let hash_hex = hex::encode(&hash);

    let tsr_bytes = fs::read(&tsr_path).await.map_err(|e| format!("Failed to read .tsr: {}", e))?;

    // Structural validation: A valid TSR is a DER-encoded TimeStampResp.
    // Minimum valid TSR is at least ~100 bytes and starts with SEQUENCE tag (0x30).
    if tsr_bytes.len() < 64 || tsr_bytes[0] != 0x30 {
        return Ok(serde_json::json!({
            "hash": hash_hex,
            "tsr_path": tsr_path,
            "tsr_size": tsr_bytes.len(),
            "integrity": "FAILED - TSR file is malformed or corrupted",
            "status": "invalid_tsr"
        }));
    }

    // Check that the file's current hash appears in the TSR's MessageImprint.
    // We look for the specific DER pattern: OCTET STRING (tag 0x04, length 0x20) followed by the 32 hash bytes.
    // This is more precise than a raw sliding-window search.
    let mut hash_verified = false;
    for i in 0..tsr_bytes.len().saturating_sub(33) {
        if tsr_bytes[i] == 0x04 && tsr_bytes[i + 1] == 0x20 && &tsr_bytes[i + 2..i + 34] == hash.as_slice() {
            hash_verified = true;
            break;
        }
    }

    let (integrity_msg, status) = if hash_verified {
        ("INTEGRITY_OK - File hash matches TSR (note: TSA signature not cryptographically verified — use 'openssl ts -verify' for full validation)", "integrity_ok")
    } else {
        ("FAILED - File has been modified since notarization", "tampered")
    };

    Ok(serde_json::json!({
        "hash": hash_hex,
        "tsr_path": tsr_path,
        "tsr_size": tsr_bytes.len(),
        "integrity": integrity_msg,
        "status": status
    }))
}

fn push_der_length(buf: &mut Vec<u8>, len: usize) {
    if len < 128 {
        buf.push(len as u8);
    } else if len < 256 {
        buf.push(0x81);
        buf.push(len as u8);
    } else {
        buf.push(0x82);
        buf.push((len >> 8) as u8);
        buf.push((len & 0xFF) as u8);
    }
}
