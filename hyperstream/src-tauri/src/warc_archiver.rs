use std::path::PathBuf;
use rquest::Client;
use chrono::Utc;
use uuid::Uuid;
use tokio::fs;

pub async fn download_as_warc(url: String, save_path: PathBuf) -> Result<String, String> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) width/1920 HyperStream/1.0")
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

    let body_bytes = response.bytes().await.map_err(|e| format!("Failed to read body: {}", e))?;
    let content_length = http_headers.len() + body_bytes.len();

    let date_now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
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
    let req_http = format!("GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0\r\n\r\n", 
        url, 
        reqwest::Url::parse(&url).map(|u| u.host_str().unwrap_or("").to_string()).unwrap_or_default()
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
