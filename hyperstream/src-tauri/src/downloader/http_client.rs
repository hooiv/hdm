//! HTTP Client with First Byte Scout
//! 
//! Implements "First Byte Scout" pattern - the first thread tests the server's
//! behavior before spawning multiple threads. This prevents downloading the
//! file 8 times if the server ignores Range requests.

use reqwest::{Client, Response, StatusCode};
use reqwest::header::{HeaderMap, HeaderValue, RANGE, ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT, REFERER};
use std::time::Duration;

use crate::downloader::network::{RetryStrategy, analyze_status, is_captive_portal};
use serde::Serialize;

/// Server capability flags determined by First Byte Scout
#[derive(Debug, Clone, Default, Serialize)]
pub struct ServerCapabilities {
    /// Server supports Range requests
    pub supports_range: bool,
    /// Server is returning the correct file (not error page)
    pub valid_content: bool,
    /// Total file size
    pub content_length: Option<u64>,
    /// Content type
    pub content_type: Option<String>,
    /// ETag for validation
    pub etag: Option<String>,
    /// Last-Modified for validation
    pub last_modified: Option<String>,
    /// Maximum segments recommended
    pub recommended_segments: u32,
    /// Server returned 200 instead of 206 (ignoring Range)
    pub ignores_range: bool,
}

/// Configuration for the HTTP client
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Request timeout
    pub timeout: Duration,
    /// Connect timeout
    pub connect_timeout: Duration,
    /// User agent string
    pub user_agent: String,
    /// Follow redirects
    pub follow_redirects: bool,
    /// Maximum redirects to follow
    pub max_redirects: usize,
    /// Accept invalid SSL certificates (for testing only!)
    pub danger_accept_invalid_certs: bool,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            user_agent: format!("HyperStream/1.0 (Windows; Rust)"),
            follow_redirects: true,
            max_redirects: 10,
            danger_accept_invalid_certs: true, // TODO: Make this configurable
        }
    }
}

/// Chrome-like user agent for better compatibility
pub const CHROME_USER_AGENT: &str = 
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Build a configured HTTP client
pub fn build_client(config: &HttpClientConfig) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder()
        .timeout(config.timeout)
        .connect_timeout(config.connect_timeout)
        .user_agent(&config.user_agent)
        .redirect(if config.follow_redirects {
            reqwest::redirect::Policy::limited(config.max_redirects)
        } else {
            reqwest::redirect::Policy::none()
        });

    if config.danger_accept_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
    }

    builder.build()
}

/// Build a client with Chrome-like headers for bypassing basic bot detection
pub fn build_stealth_client(config: &HttpClientConfig) -> Result<Client, reqwest::Error> {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(USER_AGENT, HeaderValue::from_static(CHROME_USER_AGENT));
    default_headers.insert("Accept", HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8"));
    default_headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.9"));
    default_headers.insert("Accept-Encoding", HeaderValue::from_static("identity")); // Disable compression for accurate byte counting
    default_headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    default_headers.insert("Sec-Ch-Ua", HeaderValue::from_static("\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\""));
    default_headers.insert("Sec-Ch-Ua-Mobile", HeaderValue::from_static("?0"));
    default_headers.insert("Sec-Ch-Ua-Platform", HeaderValue::from_static("\"Windows\""));
    default_headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("document"));
    default_headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("navigate"));
    default_headers.insert("Sec-Fetch-Site", HeaderValue::from_static("none"));
    default_headers.insert("Sec-Fetch-User", HeaderValue::from_static("?1"));
    default_headers.insert("Upgrade-Insecure-Requests", HeaderValue::from_static("1"));

    let mut builder = Client::builder()
        .timeout(config.timeout)
        .connect_timeout(config.connect_timeout)
        .default_headers(default_headers)
        .redirect(if config.follow_redirects {
            reqwest::redirect::Policy::limited(config.max_redirects)
        } else {
            reqwest::redirect::Policy::none()
        });

    if config.danger_accept_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
    }

    builder.build()
}

/// First Byte Scout - Probe the server to determine its capabilities
pub struct FirstByteScout {
    client: Client,
}

impl FirstByteScout {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Probe the server with a HEAD request first, then a small Range request
    pub async fn probe(&self, url: &str) -> Result<ServerCapabilities, String> {
        // Step 1: Send HEAD request to get file info
        let head_result = self.send_head_request(url).await?;
        
        // Step 2: If HEAD succeeded, verify with a small Range request
        let capabilities = self.verify_range_support(url, &head_result).await?;
        
        Ok(capabilities)
    }

    async fn send_head_request(&self, url: &str) -> Result<ServerCapabilities, String> {
        let response = self.client
            .head(url)
            .header(USER_AGENT, CHROME_USER_AGENT)
            .send()
            .await
            .map_err(|e| format!("HEAD request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Server returned: {}", response.status()));
        }

        let headers = response.headers();
        
        let content_length = headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let accept_ranges = headers
            .get(ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok());

        let supports_range = accept_ranges.map(|v| v != "none").unwrap_or(true);

        let etag = headers
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let last_modified = headers
            .get("Last-Modified")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        // Determine recommended segments based on file size
        let recommended_segments = match content_length {
            Some(size) if size > 100_000_000 => 16, // >100MB: 16 segments
            Some(size) if size > 10_000_000 => 8,  // >10MB: 8 segments
            Some(size) if size > 1_000_000 => 4,   // >1MB: 4 segments
            _ => 1, // Small files: single thread
        };

        Ok(ServerCapabilities {
            supports_range,
            valid_content: true,
            content_length,
            content_type,
            etag,
            last_modified,
            recommended_segments,
            ignores_range: false,
        })
    }

    /// Verify range support by requesting first 1KB
    async fn verify_range_support(&self, url: &str, initial: &ServerCapabilities) -> Result<ServerCapabilities, String> {
        if !initial.supports_range {
            return Ok(initial.clone());
        }

        // Request first 1KB to verify Range support
        let response = self.client
            .get(url)
            .header(USER_AGENT, CHROME_USER_AGENT)
            .header(RANGE, "bytes=0-1023")
            .send()
            .await
            .map_err(|e| format!("Range verification failed: {}", e))?;

        let status = response.status();
        let headers = response.headers().clone();

        // Get first bytes to check for captive portal
        let bytes = response.bytes().await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Check for captive portal / error page
        if is_captive_portal(&bytes) {
            return Err("Detected captive portal or error page".to_string());
        }

        let mut caps = initial.clone();

        match status {
            StatusCode::PARTIAL_CONTENT => {
                // Server properly supports Range - great!
                caps.supports_range = true;
                caps.ignores_range = false;
            }
            StatusCode::OK => {
                // Server returned 200 instead of 206 - it's ignoring Range header
                // This means we must download single-threaded
                caps.supports_range = false;
                caps.ignores_range = true;
                caps.recommended_segments = 1;
                println!("[Scout] Warning: Server ignores Range requests, falling back to single-threaded");
            }
            _ => {
                return Err(format!("Unexpected status during Range verification: {}", status));
            }
        }

        // Update content length if provided in Content-Range
        if let Some(range_header) = headers.get("Content-Range") {
            if let Ok(range_str) = range_header.to_str() {
                // Format: "bytes 0-1023/total_size"
                if let Some(total) = range_str.split('/').last() {
                    if let Ok(size) = total.parse::<u64>() {
                        caps.content_length = Some(size);
                    }
                }
            }
        }

        Ok(caps)
    }
}

/// Start a range request for a specific byte range
#[allow(dead_code)]
pub async fn start_range_request(
    client: &Client,
    url: &str,
    start: u64,
    end: u64,
    referer: Option<&str>,
) -> Result<Response, reqwest::Error> {
    let range = format!("bytes={}-{}", start, end - 1);
    
    let mut request = client
        .get(url)
        .header(USER_AGENT, CHROME_USER_AGENT)
        .header(RANGE, range);

    if let Some(ref_url) = referer {
        request = request.header(REFERER, ref_url);
    }

    request.send().await
}

/// Validate that a response is suitable for downloading
#[allow(dead_code)]
pub fn validate_download_response(response: &Response, expected_size: Option<u64>) -> Result<(), String> {
    let status = response.status();
    
    // Must be 200 or 206
    if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
        let strategy = analyze_status(status);
        return Err(match strategy {
            RetryStrategy::Fatal(msg) => msg,
            _ => format!("Server returned: {}", status),
        });
    }

    // Check content type for error pages
    if let Some(ct) = response.headers().get(CONTENT_TYPE) {
        if let Ok(ct_str) = ct.to_str() {
            if ct_str.contains("text/html") && expected_size.map(|s| s > 1024).unwrap_or(true) {
                return Err("Received HTML instead of expected file".to_string());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_client() {
        let config = HttpClientConfig::default();
        let client = build_client(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_stealth_client() {
        let config = HttpClientConfig::default();
        let client = build_stealth_client(&config);
        assert!(client.is_ok());
    }
}
