use reqwest::Client;
use std::path::Path;
use std::time::Duration;

/// List of public IPFS gateways to try (fastest first).
const GATEWAYS: &[&str] = &[
    "https://ipfs.io/ipfs/",
    "https://gateway.pinata.cloud/ipfs/",
    "https://cloudflare-ipfs.com/ipfs/",
    "https://dweb.link/ipfs/",
    "https://w3s.link/ipfs/",
];

/// Resolve an IPFS CID to the fastest responding public gateway URL.
pub async fn resolve_ipfs_gateway(cid: &str) -> Result<String, String> {
    // Validate CID contains only safe characters (alphanumeric, base-encoding chars)
    if cid.is_empty() || !cid.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(format!("Invalid IPFS CID: {}", cid));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    // Race all gateways concurrently — first success wins  
    // Use tokio::select! via a JoinSet for true first-wins semantics
    let mut set = tokio::task::JoinSet::new();
    for gateway in GATEWAYS {
        let url = format!("{}{}", gateway, cid);
        let c = client.clone();
        set.spawn(async move {
            let res = c.head(&url)
                .header("User-Agent", "HyperStream/1.0")
                .send()
                .await
                .ok()?;
            if res.status().is_success() || res.status().is_redirection() {
                Some(url)
            } else {
                None
            }
        });
    }

    // Return the first successful result, abort remaining tasks
    while let Some(result) = set.join_next().await {
        if let Ok(Some(url)) = result {
            set.abort_all();
            return Ok(url);
        }
    }

    Err(format!("No gateway could resolve CID: {}", cid))
}

/// Download content from IPFS via public gateway.
pub async fn download_ipfs(cid: String, save_path: String) -> Result<serde_json::Value, String> {
    // Validate save_path is within the download directory  
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    // For new files, canonicalize the parent directory (which must exist)
    let save_path_buf = std::path::PathBuf::from(&save_path);
    let parent = save_path_buf.parent().ok_or("Invalid save path: no parent directory")?;
    let abs_parent = dunce::canonicalize(parent)
        .map_err(|e| format!("Cannot resolve save path parent: {}", e))?;
    if !abs_parent.starts_with(&download_dir) {
        return Err("Save path must be within the download directory".to_string());
    }

    let gateway_url = resolve_ipfs_gateway(&cid).await?;

    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    let response = client.get(&gateway_url)
        .header("User-Agent", "HyperStream/1.0")
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Gateway returned status: {}", response.status()));
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    // Guard against excessively large downloads (1 GB limit for direct IPFS fetch)
    const MAX_IPFS_SIZE: u64 = 1024 * 1024 * 1024;
    if let Some(cl) = response.content_length() {
        if cl > MAX_IPFS_SIZE {
            return Err(format!("IPFS content too large ({} bytes). Use a dedicated IPFS client for files over 1 GB.", cl));
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = Path::new(&save_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Stream response to disk instead of buffering entire body in memory
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&save_path).await
        .map_err(|e| format!("Failed to create file: {}", e))?;
    let mut stream = response.bytes_stream();
    let mut file_size: u64 = 0;
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream read error: {}", e))?;
        file_size += chunk.len() as u64;
        if file_size > MAX_IPFS_SIZE {
            drop(file);
            let _ = tokio::fs::remove_file(&save_path).await;
            return Err(format!("IPFS content too large ({} bytes). Use a dedicated IPFS client for files over 1 GB.", file_size));
        }
        file.write_all(&chunk).await.map_err(|e| format!("Write error: {}", e))?;
    }
    file.flush().await.map_err(|e| format!("Flush error: {}", e))?;

    Ok(serde_json::json!({
        "status": "downloaded",
        "cid": cid,
        "gateway_url": gateway_url,
        "save_path": save_path,
        "file_size": file_size,
        "content_type": content_type,
    }))
}

/// Parse an IPFS URI into a CID.
/// Supports: ipfs://CID, /ipfs/CID, QmXXX..., baXXX...
pub fn parse_ipfs_uri(input: &str) -> Option<String> {
    let trimmed = input.trim();

    let cid = if trimmed.starts_with("ipfs://") {
        trimmed.replace("ipfs://", "").trim_start_matches('/').to_string()
    } else if trimmed.starts_with("/ipfs/") {
        trimmed.replace("/ipfs/", "").to_string()
    } else if (trimmed.starts_with("Qm") && trimmed.len() == 46) ||
              (trimmed.starts_with("ba") && trimmed.len() >= 59) {
        trimmed.to_string()
    } else {
        return None;
    };

    // Validate CID contains only safe characters (alphanumeric, -, _) — consistent with resolve_ipfs_gateway
    if !cid.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return None;
    }

    Some(cid)
}
