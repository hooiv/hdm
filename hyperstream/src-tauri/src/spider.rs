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

    /// Maximum number of pages to visit during a crawl to prevent unbounded resource consumption.
    const MAX_PAGES: usize = 500;

    pub async fn crawl(&self, options: SpiderOptions) -> Result<Vec<GrabbedFile>, String> {
        let start_url = Url::parse(&options.url).map_err(|e| e.to_string())?;

        // SSRF protection: only allow http/https schemes
        match start_url.scheme() {
            "http" | "https" => {}
            scheme => return Err(format!("Unsupported URL scheme '{}': only http and https are allowed", scheme)),
        }

        // Block private/internal IP ranges
        if let Some(host) = start_url.host_str() {
            if Self::is_private_host(host) {
                return Err(format!("Crawling private/internal addresses is not allowed: {}", host));
            }
        }

        let domain = start_url.domain().ok_or("Invalid URL domain")?.to_string();

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        queue.push_back((start_url.clone(), 0));
        visited.insert(start_url.to_string());

        // Compile regex once outside the loop for performance
        let link_regex = regex::Regex::new(r#"(?:href|src)=["']([^"']+)["']"#)
            .map_err(|e| format!("Failed to compile regex: {}", e))?;

        let mut pages_visited: usize = 0;

        while let Some((current_url, depth)) = queue.pop_front() {
            if depth > options.max_depth {
                continue;
            }

            // Enforce page count limit to prevent DoS
            pages_visited += 1;
            if pages_visited > Self::MAX_PAGES {
                break;
            }

            // SSRF protection on each resolved URL
            match current_url.scheme() {
                "http" | "https" => {}
                _ => continue,
            }
            if let Some(host) = current_url.host_str() {
                if Self::is_private_host(host) {
                    continue;
                }
            }

            println!("Crawling: {}", current_url);

            // Fetch page
            let response = match self.client.get(current_url.clone()).send().await {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Reject responses larger than 10MB to prevent memory exhaustion
            if let Some(cl) = response.content_length() {
                if cl > 10 * 1024 * 1024 {
                    continue;
                }
            }

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

            // If HTML, parse for links (bounded to 10MB)
            let bytes = match response.bytes().await {
                Ok(b) => b,
                Err(_) => continue,
            };
            if bytes.len() > 10 * 1024 * 1024 {
                continue;
            }
            let html = String::from_utf8_lossy(&bytes).to_string();

            // Simple Regex or string finding for href/src
            // Using a simple regex to avoid adding `scraper` dependency for now if not present,
            // but `regex` is usually in dependencies.
            // Let's assume we can use a basic regex for robustness.
            // Actually, we checked Cargo.toml and `regex` IS there.
            
            let re = &link_regex;
            
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

    /// Returns true if the host resolves to a private/internal IP range (SSRF protection).
    fn is_private_host(host: &str) -> bool {
        // Block common private hostnames
        let h = host.to_lowercase();
        if h == "localhost" || h.ends_with(".local") || h.ends_with(".internal") {
            return true;
        }

        // Try to parse as IP address
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            return match ip {
                std::net::IpAddr::V4(v4) => {
                    v4.is_loopback()          // 127.0.0.0/8
                    || v4.is_private()         // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                    || v4.is_link_local()      // 169.254.0.0/16 (AWS metadata etc.)
                    || v4.is_broadcast()
                    || v4.is_unspecified()
                    || v4.octets()[0] == 0     // 0.0.0.0/8
                }
                std::net::IpAddr::V6(v6) => {
                    v6.is_loopback() || v6.is_unspecified()
                }
            };
        }

        false
    }
}
