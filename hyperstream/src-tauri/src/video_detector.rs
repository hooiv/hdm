use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;

/// Detected video stream information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedStream {
    pub url: String,
    pub stream_type: StreamType,
    pub quality: Option<String>,
    pub content_type: Option<String>,
    pub estimated_size: Option<u64>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamType {
    Hls,
    Dash,
    DirectVideo,
    DirectAudio,
}

/// Content-Types that indicate streaming manifests.
const HLS_CONTENT_TYPES: &[&str] = &[
    "application/vnd.apple.mpegurl",
    "application/x-mpegurl",
    "audio/mpegurl",
    "audio/x-mpegurl",
];

const DASH_CONTENT_TYPES: &[&str] = &[
    "application/dash+xml",
    "video/vnd.mpeg.dash.mpd",
];

const VIDEO_CONTENT_TYPES: &[&str] = &[
    "video/mp4",
    "video/webm",
    "video/ogg",
    "video/x-matroska",
    "video/x-msvideo",
    "video/quicktime",
    "video/x-flv",
    "video/3gpp",
];

const AUDIO_CONTENT_TYPES: &[&str] = &[
    "audio/mpeg",
    "audio/ogg",
    "audio/aac",
    "audio/flac",
    "audio/wav",
    "audio/webm",
];

/// Probe a URL with a HEAD request to detect its media type.
pub async fn probe_url(url: &str) -> Result<Option<DetectedStream>, String> {
    let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Only HTTP(S) URLs supported".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .head(url)
        .send()
        .await
        .map_err(|e| format!("HEAD request failed: {}", e))?;

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase());

    let content_length = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    if let Some(ref ct) = content_type {
        let ct_base = ct.split(';').next().unwrap_or(ct).trim();

        if HLS_CONTENT_TYPES.iter().any(|h| ct_base == *h) {
            return Ok(Some(DetectedStream {
                url: url.to_string(),
                stream_type: StreamType::Hls,
                quality: None,
                content_type: Some(ct_base.to_string()),
                estimated_size: content_length,
                title: None,
            }));
        }

        if DASH_CONTENT_TYPES.iter().any(|d| ct_base == *d) {
            return Ok(Some(DetectedStream {
                url: url.to_string(),
                stream_type: StreamType::Dash,
                quality: None,
                content_type: Some(ct_base.to_string()),
                estimated_size: content_length,
                title: None,
            }));
        }

        if VIDEO_CONTENT_TYPES.iter().any(|v| ct_base == *v) {
            return Ok(Some(DetectedStream {
                url: url.to_string(),
                stream_type: StreamType::DirectVideo,
                quality: None,
                content_type: Some(ct_base.to_string()),
                estimated_size: content_length,
                title: None,
            }));
        }

        if AUDIO_CONTENT_TYPES.iter().any(|a| ct_base == *a) {
            return Ok(Some(DetectedStream {
                url: url.to_string(),
                stream_type: StreamType::DirectAudio,
                quality: None,
                content_type: Some(ct_base.to_string()),
                estimated_size: content_length,
                title: None,
            }));
        }
    }

    // Extension-based fallback
    let path_lower = parsed.path().to_lowercase();
    if path_lower.ends_with(".m3u8") {
        return Ok(Some(DetectedStream {
            url: url.to_string(),
            stream_type: StreamType::Hls,
            quality: None,
            content_type: content_type,
            estimated_size: content_length,
            title: None,
        }));
    }
    if path_lower.ends_with(".mpd") {
        return Ok(Some(DetectedStream {
            url: url.to_string(),
            stream_type: StreamType::Dash,
            quality: None,
            content_type: content_type,
            estimated_size: content_length,
            title: None,
        }));
    }

    let video_exts = [".mp4", ".mkv", ".webm", ".avi", ".mov", ".flv", ".wmv", ".3gp"];
    if video_exts.iter().any(|ext| path_lower.ends_with(ext)) {
        return Ok(Some(DetectedStream {
            url: url.to_string(),
            stream_type: StreamType::DirectVideo,
            quality: None,
            content_type: content_type,
            estimated_size: content_length,
            title: None,
        }));
    }

    Ok(None)
}

/// Scan a web page for embedded video/audio streams.
/// Fetches the page HTML and extracts manifest URLs and direct video links.
pub async fn scan_page_for_streams(page_url: &str) -> Result<Vec<DetectedStream>, String> {
    let parsed = Url::parse(page_url).map_err(|e| format!("Invalid URL: {}", e))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Only HTTP(S) URLs supported".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(page_url)
        .send()
        .await
        .map_err(|e| format!("GET request failed: {}", e))?;

    // Only process HTML responses
    let ct = resp.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    if !ct.contains("text/html") && !ct.contains("application/xhtml") {
        // Not an HTML page — try probe_url instead
        if let Ok(Some(stream)) = probe_url(page_url).await {
            return Ok(vec![stream]);
        }
        return Ok(Vec::new());
    }

    let body = resp.text().await.map_err(|e| format!("Failed to read body: {}", e))?;

    // Limit body size for safety
    let body = if body.len() > 2_000_000 { &body[..2_000_000] } else { &body };

    let mut streams = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();

    // Extract page title
    let title = extract_title(body);

    // 1. Find HLS manifest URLs (.m3u8)
    for url in extract_urls_by_pattern(body, &[".m3u8"]) {
        let resolved = resolve_url(page_url, &url);
        if seen_urls.insert(resolved.clone()) {
            streams.push(DetectedStream {
                url: resolved,
                stream_type: StreamType::Hls,
                quality: None,
                content_type: Some("application/vnd.apple.mpegurl".to_string()),
                estimated_size: None,
                title: title.clone(),
            });
        }
    }

    // 2. Find DASH manifest URLs (.mpd)
    for url in extract_urls_by_pattern(body, &[".mpd"]) {
        let resolved = resolve_url(page_url, &url);
        if seen_urls.insert(resolved.clone()) {
            streams.push(DetectedStream {
                url: resolved,
                stream_type: StreamType::Dash,
                quality: None,
                content_type: Some("application/dash+xml".to_string()),
                estimated_size: None,
                title: title.clone(),
            });
        }
    }

    // 3. Find <video> and <source> tags
    for url in extract_video_source_tags(body) {
        let resolved = resolve_url(page_url, &url);
        if seen_urls.insert(resolved.clone()) {
            let stype = classify_url(&resolved);
            streams.push(DetectedStream {
                url: resolved,
                stream_type: stype,
                quality: None,
                content_type: None,
                estimated_size: None,
                title: title.clone(),
            });
        }
    }

    // 4. Find og:video meta tags
    for url in extract_og_video(body) {
        let resolved = resolve_url(page_url, &url);
        if seen_urls.insert(resolved.clone()) {
            let stype = classify_url(&resolved);
            streams.push(DetectedStream {
                url: resolved,
                stream_type: stype,
                quality: None,
                content_type: None,
                estimated_size: None,
                title: title.clone(),
            });
        }
    }

    // 5. Find JSON-LD video objects
    for url in extract_json_ld_video(body) {
        let resolved = resolve_url(page_url, &url);
        if seen_urls.insert(resolved.clone()) {
            let stype = classify_url(&resolved);
            streams.push(DetectedStream {
                url: resolved,
                stream_type: stype,
                quality: None,
                content_type: None,
                estimated_size: None,
                title: title.clone(),
            });
        }
    }

    // 6. Find video URLs embedded in JavaScript (common player patterns)
    for url in extract_js_video_urls(body) {
        let resolved = resolve_url(page_url, &url);
        if seen_urls.insert(resolved.clone()) {
            let stype = classify_url(&resolved);
            streams.push(DetectedStream {
                url: resolved,
                stream_type: stype,
                quality: None,
                content_type: None,
                estimated_size: None,
                title: title.clone(),
            });
        }
    }

    Ok(streams)
}

/// Classify a URL as a specific stream type based on extension/pattern.
fn classify_url(url: &str) -> StreamType {
    let lower = url.to_lowercase();
    if lower.contains(".m3u8") {
        StreamType::Hls
    } else if lower.contains(".mpd") {
        StreamType::Dash
    } else if lower.contains(".mp3") || lower.contains(".aac") || lower.contains(".flac")
        || lower.contains(".ogg") && !lower.contains(".ogv")
    {
        StreamType::DirectAudio
    } else {
        StreamType::DirectVideo
    }
}

/// Extract <title> from HTML.
fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let tag_end = lower[start..].find('>')?;
    let content_start = start + tag_end + 1;
    let end = lower[content_start..].find("</title>")?;
    let title = html[content_start..content_start + end].trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

/// Extract URLs matching given extensions from HTML/JS source text.
fn extract_urls_by_pattern(text: &str, extensions: &[&str]) -> Vec<String> {
    let mut urls = Vec::new();
    // Match quoted strings containing the extension
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let quote = match bytes[i] {
            b'"' | b'\'' => bytes[i],
            _ => { i += 1; continue; }
        };
        i += 1;
        let start = i;
        // Find closing quote
        while i < bytes.len() && bytes[i] != quote {
            if bytes[i] == b'\\' { i += 1; } // skip escaped chars
            i += 1;
        }
        if i >= bytes.len() { break; }
        let candidate = &text[start..i];
        i += 1;

        // Check if candidate contains any target extension
        let candidate_lower = candidate.to_lowercase();
        if extensions.iter().any(|ext| candidate_lower.contains(ext)) {
            // Basic URL validation
            if (candidate.starts_with("http://") || candidate.starts_with("https://") || candidate.starts_with("/") || candidate.starts_with("./"))
                && candidate.len() < 2048
                && !candidate.contains('<')
                && !candidate.contains('>')
            {
                urls.push(candidate.to_string());
            }
        }
    }
    urls
}

/// Extract src attributes from <video> and <source> tags.
fn extract_video_source_tags(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let lower = html.to_lowercase();

    // Find <video ... src="..."> and <source ... src="...">
    for tag in &["<video", "<source"] {
        let mut pos = 0;
        while let Some(start) = lower[pos..].find(tag) {
            let abs_start = pos + start;
            let tag_end = match lower[abs_start..].find('>') {
                Some(e) => abs_start + e,
                None => break,
            };
            let tag_content = &html[abs_start..tag_end];

            // Extract src="..."
            if let Some(src) = extract_attr(tag_content, "src") {
                if !src.is_empty() && !src.starts_with("blob:") && !src.starts_with("data:") {
                    urls.push(src);
                }
            }
            pos = tag_end + 1;
        }
    }
    urls
}

/// Extract og:video content from meta tags.
fn extract_og_video(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let lower = html.to_lowercase();
    let mut pos = 0;

    while let Some(start) = lower[pos..].find("<meta") {
        let abs_start = pos + start;
        let tag_end = match lower[abs_start..].find('>') {
            Some(e) => abs_start + e,
            None => break,
        };
        let tag_content = &html[abs_start..tag_end];
        let tag_lower = &lower[abs_start..tag_end];

        if tag_lower.contains("og:video") || tag_lower.contains("twitter:player:stream") {
            if let Some(content) = extract_attr(tag_content, "content") {
                if content.starts_with("http") {
                    urls.push(content);
                }
            }
        }
        pos = tag_end + 1;
    }
    urls
}

/// Extract video URLs from JSON-LD structured data.
fn extract_json_ld_video(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let lower = html.to_lowercase();
    let mut pos = 0;

    while let Some(start) = lower[pos..].find("application/ld+json") {
        let abs_start = pos + start;
        // Find the script content
        let content_start = match lower[abs_start..].find('>') {
            Some(e) => abs_start + e + 1,
            None => break,
        };
        let content_end = match lower[content_start..].find("</script>") {
            Some(e) => content_start + e,
            None => break,
        };

        let json_text = &html[content_start..content_end];
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_text) {
            collect_video_urls_from_json(&value, &mut urls);
        }
        pos = content_end + 1;
    }
    urls
}

/// Recursively find contentUrl/embedUrl/url in JSON-LD data.
fn collect_video_urls_from_json(value: &serde_json::Value, urls: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            let is_video = map.get("@type")
                .and_then(|t| t.as_str())
                .map(|t| t.eq_ignore_ascii_case("videoobject") || t.eq_ignore_ascii_case("video"))
                .unwrap_or(false);

            if is_video {
                for key in &["contentUrl", "embedUrl", "url"] {
                    if let Some(serde_json::Value::String(u)) = map.get(*key) {
                        if u.starts_with("http") {
                            urls.push(u.clone());
                        }
                    }
                }
            }
            for v in map.values() {
                collect_video_urls_from_json(v, urls);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_video_urls_from_json(v, urls);
            }
        }
        _ => {}
    }
}

/// Extract video URLs from JavaScript source (common player configurations).
fn extract_js_video_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();

    // Common patterns in JS-based video players:
    // - "file": "https://...mp4"
    // - source: "https://...m3u8"
    // - videoUrl = "https://..."
    // - src: "https://...mp4"
    let video_exts = [".mp4", ".webm", ".m3u8", ".mpd", ".mkv", ".mov"];

    let mut combined = Vec::new();
    combined.extend_from_slice(&video_exts);

    for url in extract_urls_by_pattern(html, &combined) {
        // Additional validation for JS-extracted URLs
        if url.starts_with("http://") || url.starts_with("https://") {
            urls.push(url);
        }
    }

    urls
}

/// Extract a tag attribute value case-insensitively.
fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pattern = format!("{}=", attr_name);
    let pos = lower.find(&pattern)?;
    let after_eq = pos + pattern.len();

    if after_eq >= tag.len() { return None; }
    let bytes = tag.as_bytes();

    let (quote, start) = if bytes[after_eq] == b'"' || bytes[after_eq] == b'\'' {
        (bytes[after_eq], after_eq + 1)
    } else {
        // Unquoted attribute — read until space or >
        let end = tag[after_eq..].find(|c: char| c.is_whitespace() || c == '>')
            .map(|e| after_eq + e)
            .unwrap_or(tag.len());
        return Some(tag[after_eq..end].to_string());
    };

    let end = tag.as_bytes()[start..].iter().enumerate()
        .find(|(_, c)| **c == quote)
        .map(|(i, _)| start + i)
        .unwrap_or(tag.len());

    Some(tag[start..end].to_string())
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    if let Ok(base_url) = Url::parse(base) {
        if let Ok(resolved) = base_url.join(url) {
            return resolved.to_string();
        }
    }
    url.to_string()
}

/// Analyze intercepted network requests from a browser extension.
/// Takes a list of (url, content_type) tuples and identifies media streams.
pub fn classify_network_requests(requests: &[(String, String)]) -> Vec<DetectedStream> {
    let mut streams = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (url, content_type) in requests {
        let ct = content_type.to_lowercase();
        let ct_base = ct.split(';').next().unwrap_or(&ct).trim();

        let stream_type = if HLS_CONTENT_TYPES.iter().any(|h| ct_base == *h) {
            Some(StreamType::Hls)
        } else if DASH_CONTENT_TYPES.iter().any(|d| ct_base == *d) {
            Some(StreamType::Dash)
        } else if VIDEO_CONTENT_TYPES.iter().any(|v| ct_base == *v) {
            Some(StreamType::DirectVideo)
        } else if AUDIO_CONTENT_TYPES.iter().any(|a| ct_base == *a) {
            Some(StreamType::DirectAudio)
        } else {
            // Extension-based fallback
            let lower = url.to_lowercase();
            if lower.contains(".m3u8") {
                Some(StreamType::Hls)
            } else if lower.contains(".mpd") {
                Some(StreamType::Dash)
            } else {
                None
            }
        };

        if let Some(stype) = stream_type {
            if seen.insert(url.clone()) {
                streams.push(DetectedStream {
                    url: url.clone(),
                    stream_type: stype,
                    quality: None,
                    content_type: Some(ct_base.to_string()),
                    estimated_size: None,
                    title: None,
                });
            }
        }
    }

    streams
}
