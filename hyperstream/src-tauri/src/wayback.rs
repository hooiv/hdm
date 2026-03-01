use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaybackSnapshot {
    pub available: bool,
    pub url: String,
    pub timestamp: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
struct WaybackApiResponse {
    #[serde(default)]
    archived_snapshots: ArchivedSnapshots,
}

#[derive(Debug, Deserialize, Default)]
struct ArchivedSnapshots {
    closest: Option<ClosestSnapshot>,
}

#[derive(Debug, Deserialize)]
struct ClosestSnapshot {
    available: bool,
    url: String,
    timestamp: String,
    status: String,
}

/// Check if a URL is available in the Wayback Machine
pub async fn check_wayback(url: &str) -> Result<Option<WaybackSnapshot>, String> {
    let api_url = format!(
        "https://archive.org/wayback/available?url={}",
        urlencoding::encode(url)
    );
    
    let client = rquest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    let response = client.get(&api_url)
        .header("User-Agent", "HyperStream/1.0")
        .send()
        .await
        .map_err(|e| format!("Wayback API request failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("Wayback API returned status: {}", response.status()));
    }
    
    let body: WaybackApiResponse = response.json()
        .await
        .map_err(|e| format!("Failed to parse Wayback response: {}", e))?;
    
    match body.archived_snapshots.closest {
        Some(snapshot) if snapshot.available => {
            Ok(Some(WaybackSnapshot {
                available: true,
                url: snapshot.url,
                timestamp: snapshot.timestamp,
                status: snapshot.status,
            }))
        }
        _ => Ok(None),
    }
}

/// Get the direct download URL from the Wayback Machine
/// Converts the Wayback web URL to a raw/download-friendly URL
pub fn get_wayback_download_url(wayback_url: &str) -> String {
    // Wayback URLs look like: https://web.archive.org/web/20210101120000/https://example.com/file.zip
    // To get the raw file, add "id_" flag: https://web.archive.org/web/20210101120000id_/https://example.com/file.zip
    if wayback_url.contains("/web/") {
        // Insert "id_" before the trailing slash after the timestamp
        let parts: Vec<&str> = wayback_url.splitn(2, "/web/").collect();
        if parts.len() == 2 {
            let rest = parts[1];
            // Find the position after the timestamp (digits followed by /)
            if let Some(slash_pos) = rest.find('/') {
                let timestamp = &rest[..slash_pos];
                let original_url = &rest[slash_pos..];
                return format!("{}/web/{}id_{}", parts[0], timestamp, original_url);
            }
        }
    }
    wayback_url.to_string()
}

/// Format a Wayback timestamp (YYYYMMDDHHmmss) to human-readable
pub fn format_wayback_timestamp(timestamp: &str) -> String {
    if timestamp.len() >= 14 {
        format!(
            "{}-{}-{} {}:{}:{}",
            &timestamp[0..4],   // Year
            &timestamp[4..6],   // Month
            &timestamp[6..8],   // Day
            &timestamp[8..10],  // Hour
            &timestamp[10..12], // Minute
            &timestamp[12..14], // Second
        )
    } else {
        timestamp.to_string()
    }
}

// We need urlencoding - use a simple implementation since it's not in deps
mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut result = String::with_capacity(input.len() * 3);
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' 
                | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                _ => {
                    result.push('%');
                    result.push_str(&format!("{:02X}", byte));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_wayback_download_url() {
        let url = "https://web.archive.org/web/20210101120000/https://example.com/file.zip";
        let result = get_wayback_download_url(url);
        assert_eq!(result, "https://web.archive.org/web/20210101120000id_/https://example.com/file.zip");
    }
    
    #[test]
    fn test_format_wayback_timestamp() {
        assert_eq!(
            format_wayback_timestamp("20210315143022"),
            "2021-03-15 14:30:22"
        );
    }
    
    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("https://example.com/file.zip"), "https%3A%2F%2Fexample.com%2Ffile.zip");
    }
}
