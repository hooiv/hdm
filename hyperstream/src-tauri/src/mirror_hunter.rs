use sha2::{Sha256, Digest};
use std::path::Path;
use tokio::fs;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct MirrorResult {
    pub url: String,
    pub source: String,
    pub confidence: String,
}

/// Compute hashes and search for alternative download mirrors.
pub async fn find_mirrors(file_path: String) -> Result<serde_json::Value, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let file_bytes = fs::read(path).await.map_err(|e| format!("Failed to read file: {}", e))?;

    // Compute SHA-256
    let mut sha256_hasher = Sha256::new();
    sha256_hasher.update(&file_bytes);
    let sha256_hash = hex::encode(sha256_hasher.finalize());

    // Compute MD5
    let md5_hash = format!("{:x}", md5::compute(&file_bytes));

    let file_size = file_bytes.len();
    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let mut mirrors: Vec<MirrorResult> = Vec::new();

    // Strategy 1: Query Hash-based mirror discovery services
    // Search via common software repositories that index by hash
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    // Try archive.org search by filename + size
    if let Ok(res) = client.get(&format!(
        "https://archive.org/advancedsearch.php?q=title%3A%22{}%22&output=json&rows=5",
        urlencoding::encode(&filename)
    )).send().await {
        if let Ok(json) = res.json::<serde_json::Value>().await {
            if let Some(docs) = json.get("response").and_then(|r| r.get("docs")).and_then(|d| d.as_array()) {
                for doc in docs.iter().take(3) {
                    if let Some(identifier) = doc.get("identifier").and_then(|i| i.as_str()) {
                        mirrors.push(MirrorResult {
                            url: format!("https://archive.org/download/{}/{}", identifier, filename),
                            source: "Internet Archive".to_string(),
                            confidence: "medium".to_string(),
                        });
                    }
                }
            }
        }
    }

    // Strategy 2: GitHub releases search by filename
    if let Ok(res) = client.get(&format!(
        "https://api.github.com/search/code?q=filename:{}+path:releases",
        urlencoding::encode(&filename)
    ))
    .header("Accept", "application/vnd.github.v3+json")
    .header("User-Agent", "HyperStream/1.0")
    .send().await {
        if res.status().is_success() {
            if let Ok(json) = res.json::<serde_json::Value>().await {
                if let Some(items) = json.get("items").and_then(|i| i.as_array()) {
                    for item in items.iter().take(3) {
                        if let Some(html_url) = item.get("html_url").and_then(|u| u.as_str()) {
                            mirrors.push(MirrorResult {
                                url: html_url.to_string(),
                                source: "GitHub".to_string(),
                                confidence: "low".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Strategy 3: SourceForge mirror suggestion
    mirrors.push(MirrorResult {
        url: format!("https://sourceforge.net/projects/search/?q={}", urlencoding::encode(&filename)),
        source: "SourceForge Search".to_string(),
        confidence: "low".to_string(),
    });

    Ok(serde_json::json!({
        "sha256": sha256_hash,
        "md5": md5_hash,
        "file_size": file_size,
        "filename": filename,
        "mirrors_found": mirrors.len(),
        "mirrors": mirrors
    }))
}
