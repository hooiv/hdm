use reqwest::Client;
use std::time::{Duration, Instant};
use serde::Serialize;
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize)]
pub struct ScoutResult {
    pub url: String,
    pub status: ScoutStatus,
    pub latency_ms: u64,
    pub content_length: Option<u64>,
    pub supports_range: bool,
    pub first_kb_hash: Option<String>,
}

#[derive(Debug)]
pub struct HostMetrics {
    pub url: String,
    pub status: ScoutStatus,
    pub latency_ms: u64,
    pub confidence_percent: u8,
    pub content_length: Option<u64>,
    pub supports_range: bool,
    pub first_kb_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ScoutStatus {
    Valid,
    SizeMismatch,
    NoRangeSupport,
    HashMismatch,
    Unreachable(String),
}

#[derive(Debug, Clone)]
pub struct MirrorScout {
    client: Client,
}

impl MirrorScout {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("Mozilla/5.0 HyperStream/1.0 (MirrorScout)")
            .build()
            .unwrap_or_default();
        
        Self { client }
    }

    /// Verifies a mirror candidate against target metadata.
    pub async fn verify_mirror(
        &self, 
        url: &str, 
        expected_size: u64, 
        expected_first_kb_hash: Option<&str>
    ) -> ScoutResult {
        let start = Instant::now();
        
        // 1. HEAD request for metadata
        let head_res = match self.client.head(url).send().await {
            Ok(resp) => resp,
            Err(e) => return ScoutResult::error(url, ScoutStatus::Unreachable(e.to_string()), start.elapsed()),
        };

        let latency = start.elapsed().as_millis() as u64;
        let content_length = head_res.content_length();
        let supports_range = head_res.headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|h| h.to_str().ok())
            .map(|s| s == "bytes")
            .unwrap_or(false);

        // 2. Validate basic metadata
        if let Some(len) = content_length {
            if !roughly_equal_size(len, expected_size) {
                return ScoutResult {
                    url: url.to_string(),
                    status: ScoutStatus::SizeMismatch,
                    latency_ms: latency,
                    content_length: Some(len),
                    supports_range,
                    first_kb_hash: None,
                };
            }
        }

        if !supports_range {
             return ScoutResult {
                url: url.to_string(),
                status: ScoutStatus::NoRangeSupport,
                latency_ms: latency,
                content_length,
                supports_range: false,
                first_kb_hash: None,
            };
        }

        // 3. Optional Hash Verification (Download first 16KB)
        let mut first_kb_hash = None;
        if expected_first_kb_hash.is_some() {
            let get_res = self.client.get(url)
                .header("Range", "bytes=0-16383")
                .send()
                .await;

            if let Ok(resp) = get_res {
                let mut buffer = Vec::with_capacity(16384);
                let mut stream = resp.bytes_stream();
                use futures_util::StreamExt;
                
                while let Some(chunk_res) = stream.next().await {
                    if let Ok(chunk) = chunk_res {
                        buffer.extend_from_slice(&chunk);
                        if buffer.len() >= 16384 { break; }
                    } else { break; }
                }

                if !buffer.is_empty() {
                    let mut hasher = Sha256::new();
                    hasher.update(&buffer[..buffer.len().min(16384)]);
                    let hash_str = hex::encode(hasher.finalize());
                    first_kb_hash = Some(hash_str.clone());

                    if let Some(expected) = expected_first_kb_hash {
                        if hash_str != expected {
                            return ScoutResult {
                                url: url.to_string(),
                                status: ScoutStatus::HashMismatch,
                                latency_ms: latency,
                                content_length,
                                supports_range,
                                first_kb_hash: Some(hash_str),
                            };
                        }
                    }
                }
            }
        }

        ScoutResult {
            url: url.to_string(),
            status: ScoutStatus::Valid,
            latency_ms: latency,
            content_length,
            supports_range,
            first_kb_hash,
        }
    }
}

impl ScoutResult {
    fn error(url: &str, status: ScoutStatus, duration: Duration) -> Self {
        Self {
            url: url.to_string(),
            status,
            latency_ms: duration.as_millis() as u64,
            content_length: None,
            supports_range: false,
            first_kb_hash: None,
        }
    }
}

fn roughly_equal_size(a: u64, b: u64) -> bool {
    a == b || a.abs_diff(b) <= 4096
}
