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
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    // Race all gateways with HEAD requests
    for gateway in GATEWAYS {
        let url = format!("{}{}", gateway, cid);
        if let Ok(res) = client.head(&url)
            .header("User-Agent", "HyperStream/1.0")
            .send().await 
        {
            if res.status().is_success() || res.status().is_redirection() {
                return Ok(url);
            }
        }
    }

    Err(format!("No gateway could resolve CID: {}", cid))
}

/// Download content from IPFS via public gateway.
pub async fn download_ipfs(cid: String, save_path: String) -> Result<serde_json::Value, String> {
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

    let bytes = response.bytes().await.map_err(|e| format!("Body read error: {}", e))?;
    let file_size = bytes.len();

    // Ensure parent directory exists
    if let Some(parent) = Path::new(&save_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    tokio::fs::write(&save_path, &bytes).await
        .map_err(|e| format!("Write error: {}", e))?;

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

    if trimmed.starts_with("ipfs://") {
        return Some(trimmed.replace("ipfs://", "").trim_start_matches('/').to_string());
    }

    if trimmed.starts_with("/ipfs/") {
        return Some(trimmed.replace("/ipfs/", "").to_string());
    }

    // Raw CID (Qm... for CIDv0, ba... for CIDv1)
    if (trimmed.starts_with("Qm") && trimmed.len() == 46) ||
       (trimmed.starts_with("ba") && trimmed.len() >= 59) {
        return Some(trimmed.to_string());
    }

    None
}
