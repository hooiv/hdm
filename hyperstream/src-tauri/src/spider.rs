use reqwest::Client;
use regex::Regex;
use std::collections::HashSet;
use url::Url;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrabbedFile {
    pub url: String,
    pub filename: String,
    pub file_type: String, // "image", "video", "document", "other"
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpiderOptions {
    pub url: String,
    pub max_depth: u32,
    pub same_domain: bool,
    pub extensions: Vec<String>, // e.g., ["jpg", "png", "mp4"]
}

pub struct Spider {
    client: Client,
}

impl Spider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn crawl(&self, options: SpiderOptions) -> Result<Vec<GrabbedFile>, String> {
        let mut visited = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut results = Vec::new();
        
        let start_url = Url::parse(&options.url).map_err(|e| e.to_string())?;
        let domain = start_url.domain().map(|d| d.to_string());

        queue.push_back((start_url.clone(), 0));
        visited.insert(start_url.to_string());

        let img_regex = Regex::new(r#"<img[^>]+src=["']([^"']+)["']"#).unwrap();
        let link_regex = Regex::new(r#"<a[^>]+href=["']([^"']+)["']"#).unwrap();

        while let Some((current_url, depth)) = queue.pop_front() {
            if depth > options.max_depth {
                continue;
            }

            println!("Crawling: {}", current_url);

            let response = match self.client.get(current_url.clone()).send().await {
                Ok(r) => r,
                Err(_) => continue,
            };

            let content_type = response.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            // If it's a file we want, add to results
            if self.is_target_file(&current_url, &content_type, &options.extensions) {
                results.push(GrabbedFile {
                    url: current_url.to_string(),
                    filename: current_url.path_segments().and_then(|s| s.last()).unwrap_or("file").to_string(),
                    file_type: self.determine_type(&content_type, &current_url),
                    size: response.content_length(),
                });
                continue; // Don't parse binary files for links
            }

            // If it's HTML, parse for more links
            if content_type.contains("text/html") {
                let html = match response.text().await {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                // Find images
                for cap in img_regex.captures_iter(&html) {
                    if let Some(src) = cap.get(1) {
                        if let Ok(abs_url) = current_url.join(src.as_str()) {
                            if self.is_target_file(&abs_url, "", &options.extensions) {
                                if visited.insert(abs_url.to_string()) {
                                    results.push(GrabbedFile {
                                        url: abs_url.to_string(),
                                        filename: abs_url.path_segments().and_then(|s| s.last()).unwrap_or("image").to_string(),
                                        file_type: "image".to_string(),
                                        size: None, // We don't know size yet
                                    });
                                }
                            }
                        }
                    }
                }

                // Find links
                if depth < options.max_depth {
                    for cap in link_regex.captures_iter(&html) {
                        if let Some(href) = cap.get(1) {
                            if let Ok(abs_url) = current_url.join(href.as_str()) {
                                // Check domain constraint
                                if options.same_domain {
                                    if abs_url.domain().map(|d| d.to_string()) != domain {
                                        continue;
                                    }
                                }

                                if visited.insert(abs_url.to_string()) {
                                    queue.push_back((abs_url, depth + 1));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    fn is_target_file(&self, url: &Url, content_type: &str, extensions: &[String]) -> bool {
        let path = url.path().to_lowercase();
        
        // Check extension
        for ext in extensions {
            if path.ends_with(&format!(".{}", ext)) {
                return true;
            }
        }

        // Check content type if extension check failed
        if !content_type.is_empty() {
            if content_type.starts_with("image/") && extensions.contains(&"jpg".to_string()) { return true; }
            if content_type.starts_with("video/") && extensions.contains(&"mp4".to_string()) { return true; }
            if content_type.starts_with("audio/") && extensions.contains(&"mp3".to_string()) { return true; }
        }

        false
    }

    fn determine_type(&self, content_type: &str, url: &Url) -> String {
        if content_type.starts_with("image/") { return "image".to_string(); }
        if content_type.starts_with("video/") { return "video".to_string(); }
        if content_type.starts_with("audio/") { return "audio".to_string(); }
        
        let path = url.path().to_lowercase();
        if path.ends_with(".pdf") || path.ends_with(".doc") { return "document".to_string(); }
        if path.ends_with(".zip") || path.ends_with(".rar") { return "archive".to_string(); }
        
        "other".to_string()
    }
}
