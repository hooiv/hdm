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
    let content = reqwest::get(url).await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
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
        let mut manager = Self {
            feeds: Arc::new(Mutex::new(Vec::new())),
        };
        manager.load();
        manager
    }

    fn get_store_path() -> std::path::PathBuf {
        let mut path = std::env::current_dir().unwrap_or_default();
        path.push("feeds.json");
        path
    }

    pub fn load(&mut self) {
        if let Ok(data) = std::fs::read_to_string(Self::get_store_path()) {
            if let Ok(feeds) = serde_json::from_str(&data) {
                *self.feeds.lock().unwrap() = feeds;
            }
        }
    }

    pub fn save(&self) {
        let feeds = self.feeds.lock().unwrap();
        if let Ok(data) = serde_json::to_string_pretty(&*feeds) {
            let _ = std::fs::write(Self::get_store_path(), data);
        }
    }

    pub fn add_feed(&self, config: FeedConfig) {
        {
            let mut feeds = self.feeds.lock().unwrap();
            feeds.push(config);
        }
        self.save();
    }

    pub fn remove_feed(&self, id: &str) {
        {
            let mut feeds = self.feeds.lock().unwrap();
            feeds.retain(|f| f.id != id);
        }
        self.save();
    }

    pub fn get_feeds(&self) -> Vec<FeedConfig> {
        self.feeds.lock().unwrap().clone()
    }
}

// Global Feed Manager
lazy_static::lazy_static! {
    pub static ref FEED_MANAGER: FeedManager = FeedManager::new();
}
