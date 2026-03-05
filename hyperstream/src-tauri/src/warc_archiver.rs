use std::path::PathBuf;
use rquest::Client;
use chrono::Utc;
use uuid::Uuid;
use tokio::fs;

pub async fn download_as_warc(url: String, save_path: PathBuf) -> Result<String, String> {
    // Validate URL scheme
    let parsed_url = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed_url.scheme() {
        "http" | "https" => {}
        s => return Err(format!("Unsupported URL scheme: {}", s)),
    }

    // Validate save_path is within the download directory
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    let parent = save_path.parent().ok_or("Invalid save path: no parent directory")?;
    let _ = std::fs::create_dir_all(parent);
    let abs_parent = dunce::canonicalize(parent)
        .map_err(|e| format!("Cannot resolve save path parent: {}", e))?;
    if !abs_parent.starts_with(&download_dir) {
        return Err("Save path must be within the download directory".to_string());
    }

    // SSRF protection: block requests to private/loopback addresses
    crate::api_replay::validate_url_not_private(&url)?;

    // Capture request time BEFORE making the request (WARC spec: date of the request)
    let request_date = Utc::now();

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) width/1920 HyperStream/1.0")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))?;

    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    let version_str = "HTTP/1.1"; // rquest doesn't easily expose this as a formatted string
    
    // Format headers
    let mut http_headers = format!("{} {} {}\r\n", version_str, status.as_u16(), status.canonical_reason().unwrap_or(""));
    for (name, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            http_headers.push_str(&format!("{}: {}\r\n", name, v));
        }
    }
    http_headers.push_str("\r\n"); // End of headers

    // Guard against excessive response size (500 MB cap)
    const MAX_WARC_SIZE: u64 = 500 * 1024 * 1024;
    if let Some(cl) = response.content_length() {
        if cl > MAX_WARC_SIZE {
            return Err(format!("Response too large for WARC archive ({} bytes, max {} bytes)", cl, MAX_WARC_SIZE));
        }
    }

    let body_bytes = response.bytes().await.map_err(|e| format!("Failed to read body: {}", e))?;
    if body_bytes.len() as u64 > MAX_WARC_SIZE {
        return Err(format!("Response too large for WARC archive ({} bytes)", body_bytes.len()));
    }
    let content_length = http_headers.len() + body_bytes.len();

    let date_now = request_date.format("%Y-%m-%dT%H:%M:%SZ");
    let record_id = Uuid::new_v4();

    // WARC Response Record Header
    let mut warc_header = format!("WARC/1.0\r\n");
    warc_header.push_str("WARC-Type: response\r\n");
    warc_header.push_str(&format!("WARC-Target-URI: {}\r\n", url));
    warc_header.push_str(&format!("WARC-Date: {}\r\n", date_now));
    warc_header.push_str(&format!("WARC-Record-ID: <urn:uuid:{}>\r\n", record_id));
    warc_header.push_str("Content-Type: application/http;msgtype=response\r\n");
    warc_header.push_str(&format!("Content-Length: {}\r\n\r\n", content_length));

    let mut final_content = Vec::new();
    final_content.extend_from_slice(warc_header.as_bytes());
    final_content.extend_from_slice(http_headers.as_bytes());
    final_content.extend_from_slice(&body_bytes);
    final_content.extend_from_slice(b"\r\n\r\n"); // End of WARC record

    // Also write a request record (optional but good practice)
    let req_record_id = Uuid::new_v4();
    // Use origin-form (path + query) per HTTP/1.1 spec, not the full URL
    let (req_path, req_host) = {
        let parsed = reqwest::Url::parse(&url).ok();
        let path = parsed.as_ref()
            .map(|u| {
                let p = u.path().to_string();
                let q = u.query().map(|q| format!("?{}", q)).unwrap_or_default();
                if p.is_empty() { format!("/{}", q) } else { format!("{}{}", p, q) }
            })
            .unwrap_or_else(|| "/".to_string());
        let host = parsed.as_ref()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_default();
        (path, host)
    };
    let req_http = format!("GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0\r\n\r\n", 
        req_path, 
        req_host
    );
    let req_content_length = req_http.len();
    
    let mut req_warc_header = format!("WARC/1.0\r\n");
    req_warc_header.push_str("WARC-Type: request\r\n");
    req_warc_header.push_str(&format!("WARC-Target-URI: {}\r\n", url));
    req_warc_header.push_str(&format!("WARC-Date: {}\r\n", date_now));
    req_warc_header.push_str(&format!("WARC-Record-ID: <urn:uuid:{}>\r\n", req_record_id));
    req_warc_header.push_str("Content-Type: application/http;msgtype=request\r\n");
    req_warc_header.push_str(&format!("WARC-Concurrent-To: <urn:uuid:{}>\r\n", record_id));
    req_warc_header.push_str(&format!("Content-Length: {}\r\n\r\n", req_content_length));

    // For better form, output request then response
    let mut full_file = Vec::new();
    full_file.extend_from_slice(req_warc_header.as_bytes());
    full_file.extend_from_slice(req_http.as_bytes());
    full_file.extend_from_slice(b"\r\n\r\n");
    
    full_file.extend_from_slice(&final_content);

    fs::write(&save_path, full_file).await.map_err(|e| e.to_string())?;

    Ok(save_path.to_string_lossy().into_owned())
}
