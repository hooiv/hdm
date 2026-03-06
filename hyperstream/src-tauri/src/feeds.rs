use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use tauri::Emitter;
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
    #[serde(default)]
    pub unread_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    pub pub_date: Option<String>,
    pub description: Option<String>,
    pub read: bool,
}

// Helper used internally for path sanitization when auto-downloading
impl FeedItem {
    pub fn unique_key(&self) -> String {
        self.link.clone()
    }
}

pub struct FeedManager {
    feeds: Arc<Mutex<Vec<FeedConfig>>>,
    items: Arc<Mutex<std::collections::HashMap<String, Vec<FeedItem>>>>,
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
            items: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        manager.load();
        manager.load_items();
        manager
    }

    fn get_store_path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("hyperstream")
            .join("feeds.json")
    }

    fn get_items_store_path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("hyperstream")
            .join("feed_items.json")
    }

    pub fn load(&self) {
        let path = Self::get_store_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(feeds) = serde_json::from_str(&data) {
                *self.feeds.lock().unwrap_or_else(|e| e.into_inner()) = feeds;
            }
        }
    }

    fn load_items(&self) {
        let path = Self::get_items_store_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, Vec<FeedItem>>>(&data) {
                *self.items.lock().unwrap_or_else(|e| e.into_inner()) = map;
            }
        }
    }

    pub fn save(&self) {
        // save feeds
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
        // save items
        let items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(data) = serde_json::to_string_pretty(&*items) {
            let path = Self::get_items_store_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&path, data) {
                eprintln!("WARNING: Failed to save feed items to {}: {}", path.display(), e);
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
            feeds.push(config.clone());
        }
        // initialize items entry
        {
            let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
            items.insert(config.id.clone(), Vec::new());
        }
        self.save();
        Ok(())
    }

    pub fn remove_feed(&self, id: &str) {
        {
            let mut feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
            feeds.retain(|f| f.id != id);
        }
        {
            let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
            items.remove(id);
        }
        self.save();
    }

    pub fn get_feeds(&self) -> Vec<FeedConfig> {
        let mut feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner()).clone();
        // compute unread counts
        let items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        for feed in feeds.iter_mut() {
            if let Some(list) = items.get(&feed.id) {
                feed.unread_count = list.iter().filter(|i| !i.read).count();
            } else {
                feed.unread_count = 0;
            }
        }
        feeds
    }
}

// Global Feed Manager
lazy_static::lazy_static! {
    pub static ref FEED_MANAGER: FeedManager = FeedManager::new();
}

// -----------------------------------------
// Public polling helpers
// -----------------------------------------

impl FeedManager {
    /// Retrieve stored items for a feed
    pub fn get_items(&self, feed_id: &str) -> Vec<FeedItem> {
        self.items.lock().unwrap_or_else(|e| e.into_inner())
            .get(feed_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Mark a specific item as read and update counters
    pub fn mark_item_read(&self, feed_id: &str, link: &str) {
        let mut changed = false;
        {
            let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(list) = items.get_mut(feed_id) {
                for item in list.iter_mut() {
                    if item.link == link && !item.read {
                        item.read = true;
                        changed = true;
                        break;
                    }
                }
            }
        }
        if changed {
            // update unread count in config
            let mut feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(f) = feeds.iter_mut().find(|f| f.id == feed_id) {
                let items = self.items.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(list) = items.get(feed_id) {
                    f.unread_count = list.iter().filter(|i| !i.read).count();
                }
            }
            self.save();
        }
    }

    /// Update an existing feed's configuration
    pub fn update_feed(&self, config: FeedConfig) -> Result<(), String> {
        // Validate regex again
        if let Some(ref pattern) = config.auto_download_regex {
            regex::RegexBuilder::new(pattern)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| format!("Invalid auto-download regex: {}", e))?;
        }
        let mut feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = feeds.iter_mut().find(|f| f.id == config.id) {
            existing.url = config.url.clone();
            existing.name = config.name.clone();
            existing.refresh_interval_mins = config.refresh_interval_mins;
            existing.auto_download_regex = config.auto_download_regex.clone();
            existing.enabled = config.enabled;
            // preserve last_checked/unread_count
        } else {
            return Err("Feed not found".to_string());
        }
        self.save();
        Ok(())
    }

    /// Perform a manual refresh for a single feed (same logic as poll iteration)
    pub async fn refresh_feed(&self, app_handle: &tauri::AppHandle, feed_id: &str) -> Result<(), String> {
        // Extract feed info under the lock, then drop it before the async fetch
        let (feed_url, feed_enabled, feed_id_owned, auto_download_regex) = {
            let feeds_lock = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(feed) = feeds_lock.iter().find(|f| f.id == feed_id) {
                if !feed.enabled {
                    return Err("Feed disabled".to_string());
                }
                (feed.url.clone(), feed.enabled, feed.id.clone(), feed.auto_download_regex.clone())
            } else {
                return Err("Feed not found".to_string());
            }
        };

        // Fetch without holding any lock
        let new_items = fetch_feed(&feed_url).await?;

        // Re-acquire lock to update state
        {
            let mut feeds_lock = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(feed) = feeds_lock.iter_mut().find(|f| f.id == feed_id) {
                feed.last_checked = Some(chrono::Utc::now().timestamp());
            }
        }

        let mut added = Vec::new();
        {
            let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
            let list = items.entry(feed_id_owned.clone()).or_default();
            for item in new_items {
                if !list.iter().any(|e| e.link == item.link) {
                    added.push(item.clone());
                    list.insert(0, item);
                }
            }
        }
        if !added.is_empty() {
            // update unread count
            {
                let mut feeds_lock = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(feed) = feeds_lock.iter_mut().find(|f| f.id == feed_id) {
                    feed.unread_count += added.len();
                }
            }
            let _ = app_handle.emit("feed_update", serde_json::json!({"feed_id": feed_id_owned, "new_items": added.clone()}));
            let _ = crate::http_server::get_event_sender().send(serde_json::json!({"type":"feed_update","feed_id":feed_id_owned,"new_items":added.clone()}));
            // handle auto-download
            if let Some(ref regex_str) = auto_download_regex {
                if let Ok(re) = regex::Regex::new(regex_str) {
                    for item in added.iter() {
                        if re.is_match(&item.link) || re.is_match(&item.title) {
                            let ah = app_handle.clone();
                            let feed_id_clone = feed_id_owned.clone();
                            let item_clone = item.clone();
                            tauri::async_runtime::spawn(async move {
                                use tauri::Manager;
                                let settings = crate::settings::load_settings();
                                let filename = sanitize_filename(&item_clone.title, &item_clone.link);
                                let path = format!("{}/{}", settings.download_dir, filename);
                                let state = ah.state::<crate::core_state::AppState>();
                                let _ = crate::engine::session::start_download_impl(&ah, &state, format!("feed_{}_{}", feed_id_clone, chrono::Utc::now().timestamp()), item_clone.link.clone(), path, None, None, false).await;
                            });
                        }
                    }
                }
            }
        }
        self.save();
        Ok(())
    }

    /// Quick helper used by poller to run one iteration over all feeds.
    pub async fn poll_once(&self, app_handle: &tauri::AppHandle) {
        let now_ts = chrono::Utc::now().timestamp();
        let feeds_snapshot: Vec<FeedConfig> = {
            self.feeds.lock().unwrap_or_else(|e| e.into_inner()).clone()
        };
        for mut feed in feeds_snapshot {
            if !feed.enabled { continue; }
            let due = match feed.last_checked {
                Some(ts) => now_ts - ts >= (feed.refresh_interval_mins as i64) * 60,
                None => true,
            };
            if !due { continue; }
            if let Ok(new_items) = fetch_feed(&feed.url).await {
                // update manager state
                let mut added = Vec::new();
                {
                    let mut items_lock = self.items.lock().unwrap_or_else(|e| e.into_inner());
                    let list = items_lock.entry(feed.id.clone()).or_default();
                    for item in new_items {
                        if !list.iter().any(|e| e.link == item.link) {
                            added.push(item.clone());
                            list.insert(0, item);
                        }
                    }
                }
                if !added.is_empty() {
                    // emit notification & update counters
                    let mut feeds_lock = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(f) = feeds_lock.iter_mut().find(|f| f.id == feed.id) {
                        f.unread_count += added.len();
                        f.last_checked = Some(now_ts);
                    }
                    let _ = app_handle.emit("feed_update", serde_json::json!({"feed_id": feed.id, "new_items": added.clone()}));
                    let _ = crate::http_server::get_event_sender().send(serde_json::json!({"type":"feed_update","feed_id":feed.id,"new_items":added.clone()}));
                    // perform auto-download if regex present
                    if let Some(ref regex_str) = feed.auto_download_regex {
                        if let Ok(re) = regex::Regex::new(regex_str) {
                            for item in added.iter() {
                                if re.is_match(&item.link) || re.is_match(&item.title) {
                                    let ah = app_handle.clone();
                                    let feed_id_clone = feed.id.clone();
                                    let item_clone = item.clone();
                                    tauri::async_runtime::spawn(async move {
                                        use tauri::Manager;
                                        let settings = crate::settings::load_settings();
                                        let filename = sanitize_filename(&item_clone.title, &item_clone.link);
                                        let path = format!("{}/{}", settings.download_dir, filename);
                                        let state = ah.state::<crate::core_state::AppState>();
                                        let _ = crate::engine::session::start_download_impl(&ah, &state, format!("feed_{}_{}", feed_id_clone, chrono::Utc::now().timestamp()), item_clone.link.clone(), path, None, None, false).await;
                                    });
                                }
                            }
                        }
                    }
                    self.save();
                } else {
                    // just update last_checked because we polled
                    let mut feeds_lock = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(f) = feeds_lock.iter_mut().find(|f| f.id == feed.id) {
                        f.last_checked = Some(now_ts);
                    }
                    self.save();
                }
            }
        }
    }
}

/// Throttled polling loop started at app startup.
pub fn start_poller(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            FEED_MANAGER.poll_once(&app_handle).await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

// Helper for generating a safe filename from title and url
fn sanitize_filename(title: &str, url: &str) -> String {
    let ext = std::path::Path::new(url)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mut base: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    if !ext.is_empty() && !base.ends_with(ext) {
        base.push('.');
        base.push_str(ext);
    }
    base
}

// ----------------------------------
// Unit tests for feeds
// ----------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use warp::Filter;

    /// Start a small HTTP server returning the given body and return the bound address
    async fn serve_feed(body: &'static str) -> std::net::SocketAddr {
        let route = warp::path::end().map(move || warp::reply::with_header(body, "Content-Type", "application/rss+xml"));
        let (addr, server) = warp::serve(route).bind_ephemeral(([127, 0, 0, 1], 0));
        tokio::spawn(server);
        addr
    }

    #[tokio::test]
    async fn fetch_feed_success() {
        let xml = r#"<?xml version=\"1.0\"?><rss><channel><item><title>Foo</title><link>http://example.com</link></item></channel></rss>"#;
        let addr = serve_feed(xml).await;
        let url = format!("http://{}", addr);
        let items = fetch_feed(&url).await.expect("fetch should succeed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Foo");
        assert_eq!(items[0].link, "http://example.com");
    }

    #[tokio::test]
    async fn fetch_feed_ssrf_blocked() {
        // local host should be rejected before any network request
        let err = fetch_feed("http://127.0.0.1/feed.xml").await.unwrap_err();
        assert!(err.contains("private"));
    }

    #[test]
    fn feed_manager_add_remove_update() {
        let mgr = FeedManager::new();
        // start fresh by removing any test id
        let id = "test123".to_string();
        mgr.remove_feed(&id);

        let config = FeedConfig {
            id: id.clone(),
            url: "https://example.com/rss".to_string(),
            name: "Example".to_string(),
            refresh_interval_mins: 5,
            auto_download_regex: None,
            last_checked: None,
            enabled: true,
            unread_count: 0,
        };
        assert!(mgr.add_feed(config.clone()).is_ok());
        let feeds = mgr.get_feeds();
        assert!(feeds.iter().any(|f| f.id == id));

        // update feed name
        let mut newcfg = config.clone();
        newcfg.name = "Renamed".to_string();
        assert!(mgr.update_feed(newcfg.clone()).is_ok());
        let feeds2 = mgr.get_feeds();
        assert!(feeds2.iter().any(|f| f.name == "Renamed"));

        mgr.remove_feed(&id);
        let feeds3 = mgr.get_feeds();
        assert!(!feeds3.iter().any(|f| f.id == id));
    }
}
