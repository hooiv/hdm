use reqwest::Client;
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::task;

#[derive(Serialize, Clone, Debug)]
pub struct ProbeResult {
    pub url: String,
    pub speed_bytes_per_sec: u64,
    pub latency_ms: u64,
    pub supports_range: bool,
    pub content_length: u64,
    pub status: u16,
}

/// Race multiple mirror URLs and rank them by download speed.
/// Downloads a small probe (first 256KB) from each URL concurrently to measure real throughput.
pub async fn arbitrage_probe(urls: Vec<String>) -> Result<Vec<ProbeResult>, String> {
    if urls.is_empty() {
        return Err("No URLs provided".to_string());
    }

    let mut handles = Vec::new();

    for url in urls {
        let handle = task::spawn(async move {
            probe_mirror(&url).await
        });
        handles.push(handle);
    }

    let mut results: Vec<ProbeResult> = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => eprintln!("Probe failed: {}", e),
            Err(e) => eprintln!("Task join error: {}", e),
        }
    }

    // Sort by speed descending (fastest first)
    results.sort_by(|a, b| b.speed_bytes_per_sec.cmp(&a.speed_bytes_per_sec));

    Ok(results)
}

async fn probe_mirror(url: &str) -> Result<ProbeResult, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Client error: {}", e))?;

    // First, do a HEAD request to get content-length and range support
    let head_start = Instant::now();
    let head_res = client.head(url)
        .header("User-Agent", "Mozilla/5.0 HyperStream/1.0")
        .send()
        .await
        .map_err(|e| format!("HEAD failed for {}: {}", url, e))?;
    let latency = head_start.elapsed().as_millis() as u64;

    let status = head_res.status().as_u16();
    let content_length = head_res.headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let supports_range = head_res.headers()
        .get("accept-ranges")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("bytes"))
        .unwrap_or(false);

    // Download first 256KB to measure actual speed
    let probe_size: u64 = 256 * 1024; // 256KB probe
    let download_start = Instant::now();

    let get_res = client.get(url)
        .header("Range", format!("bytes=0-{}", probe_size - 1))
        .header("User-Agent", "Mozilla/5.0 HyperStream/1.0")
        .send()
        .await
        .map_err(|e| format!("GET probe failed for {}: {}", url, e))?;

    let body = get_res.bytes().await.map_err(|e| format!("Body read error: {}", e))?;
    let elapsed = download_start.elapsed();
    let downloaded = body.len() as u64;

    let speed = if elapsed.as_secs_f64() > 0.0 {
        (downloaded as f64 / elapsed.as_secs_f64()) as u64
    } else {
        downloaded * 1000 // very fast
    };

    Ok(ProbeResult {
        url: url.to_string(),
        speed_bytes_per_sec: speed,
        latency_ms: latency,
        supports_range,
        content_length,
        status,
    })
}

/// Convenience: pick the fastest URL from a list.
pub async fn get_fastest_mirror(urls: Vec<String>) -> Result<String, String> {
    let results = arbitrage_probe(urls).await?;
    results.first()
        .map(|r| r.url.clone())
        .ok_or_else(|| "No mirrors responded".to_string())
}
