use reqwest::Client;
use std::collections::{HashSet, VecDeque};
use url::Url;
use std::time::{Duration, Instant};

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

/// Parsed robots.txt rules for a single user-agent.
struct RobotRules {
    disallow: Vec<String>,
    allow: Vec<String>,
    crawl_delay: Option<f64>,
}

impl RobotRules {
    fn empty() -> Self {
        Self { disallow: Vec::new(), allow: Vec::new(), crawl_delay: None }
    }

    /// Parse a robots.txt body, extracting rules relevant to our user-agent.
    fn parse(body: &str) -> Self {
        let mut rules = RobotRules::empty();
        let mut in_our_section = false;
        let mut found_specific = false;
        let ua = "hyperstream";

        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let lower = line.to_lowercase();
            if lower.starts_with("user-agent:") {
                let agent = lower["user-agent:".len()..].trim().to_string();
                if agent == "*" && !found_specific {
                    in_our_section = true;
                } else if agent == ua {
                    // Reset to prefer specific rules
                    if !found_specific {
                        rules = RobotRules::empty();
                    }
                    found_specific = true;
                    in_our_section = true;
                } else {
                    if !found_specific {
                        in_our_section = false;
                    }
                }
            } else if in_our_section {
                if lower.starts_with("disallow:") {
                    let path = line["Disallow:".len()..].trim();
                    if !path.is_empty() {
                        rules.disallow.push(path.to_string());
                    }
                } else if lower.starts_with("allow:") {
                    let path = line["Allow:".len()..].trim();
                    if !path.is_empty() {
                        rules.allow.push(path.to_string());
                    }
                } else if lower.starts_with("crawl-delay:") {
                    if let Ok(d) = line["Crawl-delay:".len()..].trim().parse::<f64>() {
                        rules.crawl_delay = Some(d);
                    }
                }
            }
        }
        rules
    }

    /// Check if a path is allowed by the rules (Allow takes precedence over Disallow for longer matches).
    fn is_allowed(&self, path: &str) -> bool {
        let mut best_disallow = 0usize;
        let mut best_allow = 0usize;
        for d in &self.disallow {
            if path.starts_with(d.as_str()) && d.len() > best_disallow {
                best_disallow = d.len();
            }
        }
        for a in &self.allow {
            if path.starts_with(a.as_str()) && a.len() > best_allow {
                best_allow = a.len();
            }
        }
        if best_disallow == 0 {
            return true; // nothing disallowed
        }
        // Longer match wins; on tie, allow wins
        best_allow >= best_disallow
    }
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
    /// Minimum delay between requests to the same domain (seconds).
    const MIN_CRAWL_DELAY: f64 = 0.5;
    /// Maximum crawl-delay we'll respect (cap unreasonable values).
    const MAX_CRAWL_DELAY: f64 = 30.0;
    /// User-Agent string identifying the crawler.
    const USER_AGENT: &'static str = "HyperStream/1.0 (download-manager; +https://github.com/niconicodex/HyperStream)";

    /// Fetch and parse robots.txt for a domain. Returns empty rules on failure.
    async fn fetch_robots(&self, base_url: &Url) -> RobotRules {
        let robots_url = format!("{}://{}/robots.txt", base_url.scheme(), base_url.authority());
        match self.client
            .get(&robots_url)
            .header("User-Agent", Self::USER_AGENT)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                match resp.text().await {
                    Ok(body) => RobotRules::parse(&body),
                    Err(_) => RobotRules::empty(),
                }
            }
            _ => RobotRules::empty(), // No robots.txt or error = allow all
        }
    }

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

        // Fetch robots.txt before crawling
        let robots = self.fetch_robots(&start_url).await;
        let crawl_delay = robots.crawl_delay
            .unwrap_or(Self::MIN_CRAWL_DELAY)
            .clamp(Self::MIN_CRAWL_DELAY, Self::MAX_CRAWL_DELAY);

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        queue.push_back((start_url.clone(), 0));
        visited.insert(start_url.to_string());

        // Compile regex once outside the loop for performance
        let link_regex = regex::Regex::new(r#"(?:href|src)=["']([^"']+)["']"#)
            .map_err(|e| format!("Failed to compile regex: {}", e))?;

        let mut pages_visited: usize = 0;
        let mut last_request_time = Instant::now() - Duration::from_secs(10); // allow first request immediately

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

            // robots.txt check — skip disallowed paths
            if !robots.is_allowed(current_url.path()) {
                continue;
            }

            // Enforce crawl delay between requests
            let elapsed = last_request_time.elapsed();
            let delay = Duration::from_secs_f64(crawl_delay);
            if elapsed < delay {
                tokio::time::sleep(delay - elapsed).await;
            }

            println!("Crawling: {}", current_url);
            last_request_time = Instant::now();

            // Fetch page with proper User-Agent
            let response = match self.client
                .get(current_url.clone())
                .header("User-Agent", Self::USER_AGENT)
                .timeout(Duration::from_secs(15))
                .send()
                .await
            {
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
