use reqwest::Client;
use std::collections::{HashSet, VecDeque};
use url::Url;

#[derive(Debug, Clone, serde::Serialize)]
pub struct GrabbedFile {
    pub url: String,
    pub filename: String,
    pub size: Option<u64>,
    pub file_type: String, // "image", "video", "document", "other"
}

#[derive(Debug, Clone)]
pub struct SpiderOptions {
    pub url: String,
    pub max_depth: u32,
    pub same_domain: bool,
    pub extensions: Vec<String>,
}

pub struct Spider {
    client: Client,
}

impl Spider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn crawl(&self, options: SpiderOptions) -> Result<Vec<GrabbedFile>, String> {
        let start_url = Url::parse(&options.url).map_err(|e| e.to_string())?;
        let domain = start_url.domain().ok_or("Invalid URL domain")?.to_string();

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        queue.push_back((start_url.clone(), 0));
        visited.insert(start_url.to_string());

        while let Some((current_url, depth)) = queue.pop_front() {
            if depth > options.max_depth {
                continue;
            }

            println!("Crawling: {}", current_url);

            // Fetch page
            let response = match self.client.get(current_url.clone()).send().await {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Check Content-Type
            let content_type = response.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            // If it's not HTML, check if it's a file we want
            if !content_type.contains("text/html") {
                if let Some(ext) = current_url.path().split('.').last() {
                    if options.extensions.contains(&ext.to_lowercase()) || options.extensions.is_empty() {
                         let size = response.content_length();
                         let filename = current_url.path_segments()
                            .and_then(|s| s.last())
                            .unwrap_or("file")
                            .to_string();
                        
                         results.push(GrabbedFile {
                             url: current_url.to_string(),
                             filename,
                             size,
                             file_type: Self::detect_type(&content_type),
                         });
                    }
                }
                continue;
            }

            // If HTML, parse for links
            let html = match response.text().await {
                Ok(t) => t,
                Err(_) => continue,
            };

            // Simple Regex or string finding for href/src
            // Using a simple regex to avoid adding `scraper` dependency for now if not present,
            // but `regex` is usually in dependencies.
            // Let's assume we can use a basic regex for robustness.
            // Actually, we checked Cargo.toml and `regex` IS there.
            
            let re = regex::Regex::new(r#"(?:href|src)=["']([^"']+)["']"#).unwrap();
            
            for cap in re.captures_iter(&html) {
                if let Some(link) = cap.get(1) {
                    let link_str = link.as_str();
                    
                    // Resolve relative URLs
                    let next_url = match current_url.join(link_str) {
                        Ok(u) => u,
                        Err(_) => continue,
                    };

                    // Domain restriction
                    if options.same_domain {
                         if let Some(d) = next_url.domain() {
                             if d != domain && !d.ends_with(&format!(".{}", domain)) {
                                 continue;
                             }
                         }
                    }

                    // Avoid already visited
                    if !visited.contains(next_url.as_str()) {
                        visited.insert(next_url.to_string());
                        
                        // Check extension BEFORE queueing to decide if we should queue (if HTML) or add to result (if Asset)
                        // Actually, we treat everything as a potential crawl target if depth allows,
                        // and filter in the loop top.
                        // But we can optimize: if extension matches target, it's a result.
                        // If extension is html-like or no extension, queue it.
                        
                        let is_asset = if let Some(ext) = next_url.path().split('.').last() {
                            options.extensions.contains(&ext.to_lowercase())
                        } else {
                            false
                        };
                        
                        if is_asset {
                            // It's a target file, likely not HTML (unless user wants .html files)
                            // We can push to results directly or queue to verify headers.
                            // To be safe (get size), we queue it.
                            queue.push_back((next_url, depth + 1));
                        } else {
                            // It's a potential page
                             queue.push_back((next_url, depth + 1));
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    fn detect_type(ct: &str) -> String {
        if ct.contains("image") { "image".to_string() }
        else if ct.contains("video") { "video".to_string() }
        else if ct.contains("audio") { "audio".to_string() }
        else if ct.contains("pdf") || ct.contains("document") { "document".to_string() }
        else { "other".to_string() }
    }
}
