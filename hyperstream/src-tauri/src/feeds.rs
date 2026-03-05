use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
// use log::{info, error};
use feed_rs::parser;
// use regex::Regex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedConfig {
    pub id: String,
    pub url: String,
    pub name: String,
    pub refresh_interval_mins: u64,
    pub auto_download_regex: Option<String>,
    pub last_checked: Option<i64>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    pub pub_date: Option<String>,
    pub description: Option<String>,
    pub read: bool,
}

pub struct FeedManager {
    feeds: Arc<Mutex<Vec<FeedConfig>>>,
}



// Async function to fetch feed
pub async fn fetch_feed(url: &str) -> Result<Vec<FeedItem>, String> {
    // SSRF protection: block private/loopback addresses (both IP literals and DNS-resolved)
    crate::api_replay::validate_url_not_private(url)?;
    
    // Also resolve hostname to check DNS-resolved IPs (prevents DNS rebinding)
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let port = parsed.port_or_known_default().unwrap_or(443);
            let addr_str = format!("{}:{}", host, port);
            if let Ok(addrs) = tokio::net::lookup_host(addr_str).await {
                for addr in addrs {
                    let ip = addr.ip();
                    match ip {
                        std::net::IpAddr::V4(v4) => {
                            if v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() || v4.octets()[0] == 0 {
                                return Err(format!("Feed URL resolves to private IP {}", v4));
                            }
                        }
                        std::net::IpAddr::V6(v6) => {
                            if v6.is_loopback() || v6.is_unspecified() {
                                return Err(format!("Feed URL resolves to private IPv6 {}", v6));
                            }
                            if let Some(v4) = v6.to_ipv4_mapped() {
                                if v4.is_loopback() || v4.is_private() || v4.is_link_local() {
                                    return Err(format!("Feed URL resolves to private mapped IP {}", v4));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    // Follow redirects manually with SSRF re-validation on each hop (up to 5 hops).
    let mut current_url = url.to_string();
    let mut response = client.get(&current_url).send().await.map_err(|e| e.to_string())?;

    for _hop in 0..5 {
        if !response.status().is_redirection() {
            break;
        }
        let location = response.headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| "Redirect with no Location header".to_string())?
            .to_string();

        // Use proper URL resolution for both absolute and relative redirects
        let base = reqwest::Url::parse(&current_url).map_err(|e| e.to_string())?;
        let redirect_url = base.join(&location).map_err(|e| e.to_string())?.to_string();

        // Re-validate the redirect target against private IPs
        if let Ok(parsed) = reqwest::Url::parse(&redirect_url) {
            if let Some(host) = parsed.host_str() {
                let lower = host.to_lowercase();
                if lower == "localhost" || lower == "[::1]" {
                    return Err("Feed redirect to localhost blocked".to_string());
                }
                if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                    match ip {
                        std::net::IpAddr::V4(v4) => {
                            if v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() {
                                return Err(format!("Feed redirect to private IP {} blocked", v4));
                            }
                        }
                        std::net::IpAddr::V6(v6) => {
                            if v6.is_loopback() || v6.is_unspecified() {
                                return Err(format!("Feed redirect to private IPv6 {} blocked", v6));
                            }
                            if let Some(v4) = v6.to_ipv4_mapped() {
                                if v4.is_loopback() || v4.is_private() || v4.is_link_local() {
                                    return Err(format!("Feed redirect to private mapped IP {} blocked", v4));
                                }
                            }
                        }
                    }
                }
            }
        }

        current_url = redirect_url;
        response = client.get(&current_url).send().await.map_err(|e| e.to_string())?;
    }

    // If still a redirect after 5 hops, bail out
    if response.status().is_redirection() {
        return Err("Too many redirects (exceeded 5 hops)".to_string());
    }

    // Guard against oversized responses (max 10 MB for a feed)
    if let Some(cl) = response.content_length() {
        if cl > 10 * 1024 * 1024 {
            return Err(format!("Feed response too large: {} bytes", cl));
        }
    }
    let content = response.bytes().await.map_err(|e| e.to_string())?;
    if content.len() > 10 * 1024 * 1024 {
        return Err("Feed response exceeded 10 MB limit".to_string());
    }
    let cursor = std::io::Cursor::new(content);
    let feed = parser::parse(cursor).map_err(|e| e.to_string())?;

    let items = feed.entries.into_iter().map(|entry| {
        FeedItem {
            title: entry.title.map(|t| t.content).unwrap_or_default(),
            link: entry.links.first().map(|l| l.href.clone()).unwrap_or_default(),
            pub_date: entry.published.map(|d| d.to_rfc3339()),
            description: entry.summary.map(|s| s.content),
            read: false,
        }
    }).collect();

    Ok(items)
}

impl FeedManager {
    pub fn new() -> Self {
        let manager = Self {
            feeds: Arc::new(Mutex::new(Vec::new())),
        };
        manager.load();
        manager
    }

    fn get_store_path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("hyperstream")
            .join("feeds.json")
    }

    pub fn load(&self) {
        let path = Self::get_store_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(feeds) = serde_json::from_str(&data) {
                *self.feeds.lock().unwrap_or_else(|e| e.into_inner()) = feeds;
            }
        }
    }

    pub fn save(&self) {
        let feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(data) = serde_json::to_string_pretty(&*feeds) {
            let path = Self::get_store_path();
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&path, data) {
                eprintln!("WARNING: Failed to save feeds to {}: {}", path.display(), e);
            }
        }
    }

    pub fn add_feed(&self, config: FeedConfig) -> Result<(), String> {
        // Validate auto_download_regex if provided (ReDoS protection)
        if let Some(ref pattern) = config.auto_download_regex {
            regex::RegexBuilder::new(pattern)
                .size_limit(1 << 20) // 1 MB compiled DFA limit
                .build()
                .map_err(|e| format!("Invalid auto-download regex: {}", e))?;
        }
        {
            let mut feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
            if feeds.iter().any(|f| f.id == config.id) {
                return Err(format!("Feed with ID '{}' already exists", config.id));
            }
            feeds.push(config);
        }
        self.save();
        Ok(())
    }

    pub fn remove_feed(&self, id: &str) {
        {
            let mut feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
            feeds.retain(|f| f.id != id);
        }
        self.save();
    }

    pub fn get_feeds(&self) -> Vec<FeedConfig> {
        self.feeds.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

// Global Feed Manager
lazy_static::lazy_static! {
    pub static ref FEED_MANAGER: FeedManager = FeedManager::new();
}
