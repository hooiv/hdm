use reqwest::Client;
use std::time::Duration;

pub async fn fetch_with_ja3(url: &str, browser_profile: &str) -> Result<String, String> {
    // Validate URL scheme to prevent SSRF against non-HTTP endpoints
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {},
        s => return Err(format!("Unsupported URL scheme '{}': only http/https allowed", s)),
    }

    let user_agent = match browser_profile.to_lowercase().as_str() {
        "chrome" => "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        "firefox" => "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
        "safari" => "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15",
        _ => "HyperStream/1.0"
    };

    let client = Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build JA3 spoofing client: {}", e))?;

    let response = client.get(url)
        .header("Sec-Ch-Ua", match browser_profile {
            "chrome" => "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"",
            _ => ""
        })
        .header("Sec-Ch-Ua-Mobile", "?0")
        .header("Sec-Ch-Ua-Platform", "\"Windows\"")
        .send()
        .await
        .map_err(|e| format!("JA3 Request failed: {}", e))?;

    let status = response.status();
    // Cap response body to 10 MB to prevent OOM from malicious servers
    let content_length = response.content_length().unwrap_or(0);
    if content_length > 10 * 1024 * 1024 {
        return Err(format!("Response too large: {} bytes", content_length));
    }
    let text = response.text().await.unwrap_or_default();
    if text.len() > 10 * 1024 * 1024 {
        return Err("Response body exceeded 10 MB limit".to_string());
    }

    if status.is_success() {
        Ok(text)
    } else {
        Err(format!("Server rejected JA3 profile: {} - {}", status, text))
    }
}
