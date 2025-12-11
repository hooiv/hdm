use rquest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BrowserProfile {
    Chrome,
    Firefox,
    Safari,
    Edge,
    OkHttp,
}

impl Default for BrowserProfile {
    fn default() -> Self {
        Self::Chrome
    }
}

/// Build a client that "impersonates" a browser by setting TLS/HTTP2 params manually.
/// (rquest v5.1.0 does not include pre-defined profiles, so we build our own "lite" version)
pub fn build_impersonator_client(profile: BrowserProfile, proxy: Option<&crate::proxy::ProxyConfig>) -> Result<Client, String> {
    
    // 1. Determine User-Agent
    let user_agent = match profile {
        BrowserProfile::Chrome => "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        BrowserProfile::Firefox => "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
        BrowserProfile::Safari => "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
        BrowserProfile::Edge => "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
        BrowserProfile::OkHttp => "okhttp/4.10.0",
    };

    // 2. Build Client with BoringSSL (default in rquest 5.x)
    // TODO: Phase L2b - Add specific HTTP/2 window sizes and priority frames here using .http2_* methods
    let mut builder = Client::builder()
        .user_agent(user_agent)
        .cookie_store(true);
        // .http2_initial_stream_window_size(6 * 1024 * 1024)      // 6MB - Removed as not supported in current rquest

        // .http2_initial_connection_window_size(15 * 1024 * 1024); // 15MB - Removed as not supported

    // 3. Apply Proxy
    if let Some(config) = proxy {
        if let Some(p) = config.to_rquest_proxy() {
            builder = builder.proxy(p);
        }
    }

    // Note: rquest 5.x uses .min_tls_version(Version method) which we handle in http_client.rs if needed.
    // For now, default is good (TLS 1.2+).

    let client = builder.build().map_err(|e| format!("Client build error: {}", e))?;
    Ok(client)
}

/// Helper for standard client construction
pub fn build_client(proxy: Option<&crate::proxy::ProxyConfig>) -> Result<Client, String> {
    build_impersonator_client(BrowserProfile::default(), proxy)
}
